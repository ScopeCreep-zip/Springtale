import { createSignal, onMount } from "solid-js";
import { useI18n } from "@springtale/ui";
import type { Locale } from "@springtale/ui";
import {
  getSafetyConfig,
  saveSafetyConfig,
  setWindowTitle,
  setDisguiseActive,
  setDisguiseProfile,
  setPanicTapCount,
  type SafetyConfig,
} from "../ipc/safety";

/**
 * Safety settings page — app disguise, auto-lock, quick-hide.
 *
 * Per `docs/intended-arch/ARCHITECTURE.md §2.8` (IPV threat model):
 * - Default window title is "Notes" (disguise-first)
 * - Content protection prevents screenshots/screen recordings
 * - Auto-lock timeout defaults to 5 minutes
 * - Quick-hide shortcut for instant minimize
 *
 * G5d additions: persisted disguise profile (`disguise_app_name`,
 * `disguise_icon_id`), an explicit `disguise_active` toggle that
 * survives restart, and the panic-tap threshold the platform shell
 * uses to detect rapid title-bar taps. The focused-update IPC
 * methods (`setDisguiseActive`, `setDisguiseProfile`,
 * `setPanicTapCount`) avoid the lost-update race the full-config
 * save path would hit if two tabs edited safety simultaneously.
 *
 * Settings persist to SQLite (not vault) — loads before unlock.
 *
 * **Integration status:** this page is not yet routed from the
 * colony canvas's AppSettingsPanel. The functionality (with the
 * G5d additions) is the spec'd IPV duress surface from
 * `.claude/phases/phase-2b.md`; the colony-canvas integration is a
 * separate UX task (the page renders inside a colony overlay or as
 * a SAFETY section in AppSettingsPanel — to be decided in the
 * colony-canvas surface design pass).
 */
export function SafetyPage() {
  const { t, locale, setLocale } = useI18n();
  const [config, setConfig] = createSignal<SafetyConfig>({
    window_title: "Notes",
    auto_lock_minutes: 5,
    content_protected: true,
    quick_hide_shortcut: "Ctrl+Shift+H",
    disguise_app_name: "Notes",
    disguise_icon_id: "notes",
    disguise_active: false,
    panic_tap_count: 5,
  });
  const [saved, setSaved] = createSignal(false);
  const [error, setError] = createSignal("");

  onMount(async () => {
    try {
      setConfig(await getSafetyConfig());
    } catch {
      // First run — use defaults
    }
  });

  const save = async () => {
    try {
      await saveSafetyConfig(config());
      await setWindowTitle(config().window_title);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  const updateConfig = (updates: Partial<SafetyConfig>) => {
    setConfig((prev) => ({ ...prev, ...updates }));
  };

  /**
   * G5d — flip `disguise_active` via the focused-update endpoint so a
   * concurrent edit elsewhere (e.g. a panic-tap firing) can't
   * silently overwrite this toggle on a stale full-config save.
   */
  const toggleDisguise = async () => {
    try {
      const next = !config().disguise_active;
      const persisted = await setDisguiseActive(next);
      updateConfig({ disguise_active: persisted });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  /**
   * G5d — atomically swap disguise profile. The platform shell
   * (tray icon, launcher name) consumes `disguise_icon_id` to pick
   * a resource set; the backend doesn't ship the icons.
   */
  const applyDisguiseProfile = async (appName: string, iconId: string) => {
    try {
      await setDisguiseProfile(appName, iconId);
      updateConfig({ disguise_app_name: appName, disguise_icon_id: iconId });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  /**
   * G5d — bound-checked panic-tap threshold. Backend rejects out-of-
   * range values; we surface the rejection as an inline error rather
   * than a silent revert.
   */
  const updatePanicTaps = async (raw: string) => {
    const parsed = parseInt(raw, 10);
    const count = Number.isFinite(parsed) ? parsed : 5;
    try {
      const persisted = await setPanicTapCount(count);
      updateConfig({ panic_tap_count: persisted });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("safety.title")}</h1>
      {error() && (
        <div role="alert" aria-live="assertive" class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
          {error()}
        </div>
      )}

      <div class="mt-6 max-w-md space-y-6">
        <section aria-labelledby="disguise-heading">
          <h2 id="disguise-heading" class="text-lg font-semibold text-gray-200">
            {t("safety.appDisguise")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            {t("safety.appDisguiseDesc")}
          </p>
          <div class="mt-3">
            <label for="window-title" class="sr-only">{t("safety.appDisguise")}</label>
            <select
              id="window-title"
              value={config().window_title}
              onChange={(e) => updateConfig({ window_title: e.currentTarget.value })}
              class="w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white"
            >
              <option value="Notes">{t("safety.appDisguiseNotes")}</option>
              <option value="Calculator">{t("safety.appDisguiseCalculator")}</option>
              <option value="Springtale">{t("safety.appDisguiseSpringtale")}</option>
            </select>
          </div>

          {/* G5d — persisted disguise profile (app name + icon id) */}
          <div class="mt-4 grid grid-cols-2 gap-2">
            <div>
              <label for="disguise-app-name" class="block text-sm font-medium text-gray-300">
                Launcher name
              </label>
              <input
                id="disguise-app-name"
                type="text"
                value={config().disguise_app_name}
                onInput={(e) =>
                  applyDisguiseProfile(e.currentTarget.value, config().disguise_icon_id)
                }
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white"
              />
            </div>
            <div>
              <label for="disguise-icon-id" class="block text-sm font-medium text-gray-300">
                Icon set
              </label>
              <select
                id="disguise-icon-id"
                value={config().disguise_icon_id}
                onChange={(e) =>
                  applyDisguiseProfile(config().disguise_app_name, e.currentTarget.value)
                }
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white"
              >
                <option value="notes">Notes</option>
                <option value="calculator">Calculator</option>
                <option value="files">Files</option>
              </select>
            </div>
          </div>

          {/* G5d — explicit disguise on/off, persisted across restart */}
          <div class="mt-3 flex items-center gap-3">
            <input
              id="disguise-active"
              type="checkbox"
              checked={config().disguise_active}
              onChange={toggleDisguise}
              class="h-4 w-4 rounded border-gray-700 bg-gray-800 text-blue-600"
            />
            <label for="disguise-active" class="text-sm text-gray-300">
              Display disguise now (survives restart)
            </label>
          </div>
        </section>

        <section aria-labelledby="autolock-heading">
          <h2 id="autolock-heading" class="text-lg font-semibold text-gray-200">
            {t("safety.autoLock")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            {t("safety.autoLockDesc")}
          </p>
          <div class="mt-3">
            <label for="autolock-minutes" class="block text-sm font-medium text-gray-300">
              {t("safety.autoLockMinutes")}
            </label>
            <input
              id="autolock-minutes"
              type="number"
              min="0"
              value={config().auto_lock_minutes}
              onInput={(e) => updateConfig({ auto_lock_minutes: parseInt(e.currentTarget.value) || 0 })}
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
            />
          </div>
        </section>

        <section aria-labelledby="quickhide-heading">
          <h2 id="quickhide-heading" class="text-lg font-semibold text-gray-200">
            {t("safety.quickHide")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            {t("safety.quickHideDesc")}
          </p>
          <div class="mt-3">
            <p class="rounded border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-gray-300">
              {config().quick_hide_shortcut}
            </p>
          </div>
        </section>

        <section aria-labelledby="content-protection-heading">
          <h2 id="content-protection-heading" class="text-lg font-semibold text-gray-200">
            {t("safety.contentProtection")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            {t("safety.contentProtectionDesc")}
          </p>
          <div class="mt-3 flex items-center gap-3">
            <input
              id="content-protected"
              type="checkbox"
              checked={config().content_protected}
              onChange={(e) => updateConfig({ content_protected: e.currentTarget.checked })}
              class="h-4 w-4 rounded border-gray-700 bg-gray-800 text-blue-600"
            />
            <label for="content-protected" class="text-sm text-gray-300">
              {t("safety.contentProtection")}
            </label>
          </div>
        </section>

        {/* G5d — panic-tap threshold (rapid title-bar taps trigger wipe) */}
        <section aria-labelledby="panic-tap-heading">
          <h2 id="panic-tap-heading" class="text-lg font-semibold text-gray-200">
            Panic-tap threshold
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            Rapid title-bar taps that trigger panic wipe. <strong>0</strong> disables
            the gesture; valid range <strong>0–10</strong>. The platform shell
            counts taps within a short window.
          </p>
          <div class="mt-3">
            <label for="panic-tap-count" class="sr-only">Panic-tap count</label>
            <input
              id="panic-tap-count"
              type="number"
              min="0"
              max="10"
              value={config().panic_tap_count}
              onInput={(e) => updatePanicTaps(e.currentTarget.value)}
              class="w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white"
            />
          </div>
        </section>

        <section aria-labelledby="language-heading">
          <h2 id="language-heading" class="text-lg font-semibold text-gray-200">
            {t("settings.language")}
          </h2>
          <div class="mt-3">
            <label for="dt-language-select" class="sr-only">{t("settings.language")}</label>
            <select
              id="dt-language-select"
              value={locale()}
              onChange={(e) => setLocale(e.currentTarget.value as Locale)}
              class="w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white"
            >
              <option value="en">English</option>
              <option value="es">Español</option>
              <option value="pt">Português</option>
              <option value="fr">Français</option>
              <option value="ar">العربية</option>
              <option value="th">ไทย</option>
              <option value="tl">Tagalog</option>
              <option value="ja">日本語</option>
            </select>
          </div>
        </section>

        <button
          onClick={save}
          class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
        >
          {saved() ? t("common.saved") : t("safety.saveSafety")}
        </button>

        <section aria-labelledby="panic-heading" class="border-t border-gray-800 pt-6">
          <h2 id="panic-heading" class="text-lg font-semibold text-red-400">
            {t("safety.panicWipe")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            {t("safety.panicWipeDesc")}
          </p>
          <div class="mt-3">
            <button
              onClick={async () => {
                try {
                  const { ask } = await import("@tauri-apps/plugin-dialog");
                  const confirmed = await ask(
                    t("safety.panicWipeConfirm"),
                    { title: t("safety.panicWipe"), kind: "warning" },
                  );
                  if (confirmed) {
                    const { panicWipe } = await import("../ipc/panic");
                    await panicWipe();
                    // App exits after wipe — this line may not execute
                  }
                } catch (e) {
                  setError(String(e));
                }
              }}
              class="rounded bg-red-700 px-4 py-2 text-sm font-medium text-white hover:bg-red-600"
            >
              {t("safety.panicWipeButton")}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
