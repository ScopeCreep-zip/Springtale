import { createSignal, onMount } from "solid-js";
import { useI18n } from "@springtale/ui";
import type { Locale } from "@springtale/ui";
import {
  getSafetyConfig,
  saveSafetyConfig,
  setWindowTitle,
  type SafetyConfig,
} from "../ipc/safety";

/**
 * Safety settings page — app disguise, auto-lock, quick-hide.
 *
 * Per ARCHITECTURE.md §2.8 (IPV threat model):
 * - Default window title is "Notes" (disguise-first)
 * - Content protection prevents screenshots/screen recordings
 * - Auto-lock timeout defaults to 5 minutes
 * - Quick-hide shortcut for instant minimize
 *
 * Settings persist to SQLite (not vault) — loads before unlock.
 */
export function SafetyPage() {
  const { t, locale, setLocale } = useI18n();
  const [config, setConfig] = createSignal<SafetyConfig>({
    window_title: "Notes",
    auto_lock_minutes: 5,
    content_protected: true,
    quick_hide_shortcut: "Ctrl+Shift+H",
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
