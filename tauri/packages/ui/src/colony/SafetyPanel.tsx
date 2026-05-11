/**
 * SafetyPanel — IPV duress surface inside the colony shell.
 *
 * Per `docs/intended-arch/ARCHITECTURE.md §2.8` + Phase 2b's
 * "Safety Features" spec. Surfaces the SafetyConfig disguise and
 * panic-tap controls through the colony overlay pattern (same shape
 * as `MemberPickerOverlay`, `AiConfigPanel`, etc.).
 *
 * The provider exposes both the legacy full-config get/save and the
 * G5d focused-update methods. Disguise active / disguise profile /
 * panic-tap count flow through the focused endpoints so a concurrent
 * edit elsewhere can't overwrite a stale full-config save. Auto-lock
 * / content-protection / window-title still ride the full-config
 * path because they need an explicit "Save" gesture from the
 * survivor (avoid leaking partial drafts in a tense moment).
 */
import { createResource, createSignal, Show } from "solid-js";
import type { Component } from "solid-js";
import { useDashboard } from "../dashboard/context";
import { useI18n } from "../i18n/context";

/** Mirrors `springtale_store::SafetyConfigRow` — keep in sync. */
interface SafetyConfig {
  window_title: string;
  auto_lock_minutes: number;
  content_protected: boolean;
  quick_hide_shortcut: string;
  disguise_app_name: string;
  disguise_icon_id: string;
  disguise_active: boolean;
  panic_tap_count: number;
}

const DEFAULT_CONFIG: SafetyConfig = {
  window_title: "Notes",
  auto_lock_minutes: 5,
  content_protected: true,
  quick_hide_shortcut: "Ctrl+Shift+H",
  disguise_app_name: "Notes",
  disguise_icon_id: "notes",
  disguise_active: false,
  panic_tap_count: 5,
};

export interface SafetyPanelProps {
  /** Close the overlay. */
  onClose: () => void;
  /** Optional: fired when the survivor invokes panic-wipe; host
   *  shows a confirmation modal before calling the panic op. */
  onPanicWipe?: () => Promise<void>;
  /**
   * G5f — optional hook the host App uses to apply the new
   * disguise state to the platform shell (window title, tray icon,
   * iOS alternate icon, Android launcher alias). The panel doesn't
   * know what platform it's on; the host owns that translation.
   * Called after any disguise-related backend write succeeds.
   */
  onDisguiseStateChanged?: () => Promise<void>;
  /**
   * G5h — open the travel-mode page (encrypted backup + local wipe
   * + restore from backup; per §2.6 border-crossing threat model).
   * Optional so embeddings without travel-mode plumbing still
   * compile — when omitted, the "Travel mode…" button hides.
   */
  onOpenTravelMode?: () => void;
}

export const SafetyPanel: Component<SafetyPanelProps> = (props) => {
  const db = useDashboard();
  const { t } = useI18n();
  const [config, setConfig] = createSignal<SafetyConfig>(DEFAULT_CONFIG);
  const [saved, setSaved] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Initial fetch via the existing get-config provider. The
  // provider returns `unknown` for arbitrary config keys, so we
  // narrow at the call site.
  createResource(async () => {
    try {
      const raw = (await db.provider.getConfig("safety")) as Partial<SafetyConfig> | null;
      if (raw) {
        setConfig({ ...DEFAULT_CONFIG, ...raw });
      }
    } catch (e) {
      setError(String(e));
    }
  });

  const update = (patch: Partial<SafetyConfig>) => {
    setConfig((prev) => ({ ...prev, ...patch }));
  };

  /** G5d — focused-update path; survives concurrent edits.
   *  G5f — also fires `onDisguiseStateChanged` so the host App can
   *  apply the change to the platform shell (window title, tray,
   *  iOS alternate icon, etc.) before the user sees stale chrome. */
  const toggleDisguise = async () => {
    try {
      const next = !config().disguise_active;
      const persisted = await db.provider.setDisguiseActive(next);
      update({ disguise_active: persisted });
      await props.onDisguiseStateChanged?.();
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  /** G5d — atomic two-field disguise-profile swap.
   *  G5f — host applies the new profile to the shell on success. */
  const applyDisguiseProfile = async (appName: string, iconId: string) => {
    try {
      await db.provider.setDisguiseProfile(appName, iconId);
      update({ disguise_app_name: appName, disguise_icon_id: iconId });
      await props.onDisguiseStateChanged?.();
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  /** G5d — backend bound-checks [0, 10]; surface rejection inline. */
  const updatePanicTaps = async (raw: string) => {
    const parsed = parseInt(raw, 10);
    const count = Number.isFinite(parsed) ? parsed : 5;
    try {
      const persisted = await db.provider.setPanicTapCount(count);
      update({ panic_tap_count: persisted });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  /** Full-config save for the explicit-gesture fields. */
  const save = async () => {
    try {
      // setConfig persists via /config set under "safety" key —
      // matches what the existing AppSettingsPanel patterns do for
      // other config blocks.
      await db.provider.setConfig("safety", config());
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
      <div class="mb-4 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">
          {t("safety.title")}
        </h2>
        <button onClick={props.onClose} class="colony-close-btn">✕</button>
      </div>

      <Show when={error()}>
        <div role="alert" aria-live="assertive"
             class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error">
          {error()}
        </div>
      </Show>

      <div class="space-y-4">
        {/* G5d — app disguise */}
        <section aria-labelledby="disguise-h">
          <h3 id="disguise-h" class="colony-label mb-1">{t("safety.appDisguise")}</h3>
          <p class="colony-text-3xs mb-2 text-text-dim">{t("safety.appDisguiseDesc")}</p>

          <label for="sp-window-title" class="colony-text-2xs block text-text-secondary">
            Window title
          </label>
          <select
            id="sp-window-title"
            value={config().window_title}
            onChange={(e) => update({ window_title: e.currentTarget.value })}
            class="colony-text-2xs mt-1 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
          >
            <option value="Notes">{t("safety.appDisguiseNotes")}</option>
            <option value="Calculator">{t("safety.appDisguiseCalculator")}</option>
            <option value="Springtale">{t("safety.appDisguiseSpringtale")}</option>
          </select>

          <div class="mt-3 grid grid-cols-2 gap-2">
            <div>
              <label for="sp-app-name" class="colony-text-2xs block text-text-secondary">
                Launcher name
              </label>
              <input
                id="sp-app-name"
                type="text"
                value={config().disguise_app_name}
                onInput={(e) =>
                  applyDisguiseProfile(e.currentTarget.value, config().disguise_icon_id)
                }
                class="colony-text-2xs mt-1 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
              />
            </div>
            <div>
              <label for="sp-icon-id" class="colony-text-2xs block text-text-secondary">
                Icon set
              </label>
              <select
                id="sp-icon-id"
                value={config().disguise_icon_id}
                onChange={(e) =>
                  applyDisguiseProfile(config().disguise_app_name, e.currentTarget.value)
                }
                class="colony-text-2xs mt-1 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
              >
                <option value="notes">Notes</option>
                <option value="calculator">Calculator</option>
                <option value="files">Files</option>
              </select>
            </div>
          </div>

          <div class="mt-3 flex items-center gap-2">
            <input
              id="sp-disguise-active"
              type="checkbox"
              checked={config().disguise_active}
              onChange={toggleDisguise}
              class="h-4 w-4"
            />
            <label for="sp-disguise-active" class="colony-text-2xs text-text-secondary">
              Display disguise now (persists across restart)
            </label>
          </div>
        </section>

        {/* Auto-lock */}
        <section aria-labelledby="autolock-h">
          <h3 id="autolock-h" class="colony-label mb-1">{t("safety.autoLock")}</h3>
          <p class="colony-text-3xs mb-2 text-text-dim">{t("safety.autoLockDesc")}</p>
          <label for="sp-autolock" class="colony-text-2xs block text-text-secondary">
            {t("safety.autoLockMinutes")}
          </label>
          <input
            id="sp-autolock"
            type="number"
            min="0"
            value={config().auto_lock_minutes}
            onInput={(e) => update({ auto_lock_minutes: parseInt(e.currentTarget.value, 10) || 0 })}
            class="colony-text-2xs mt-1 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
          />
        </section>

        {/* Content protection */}
        <section aria-labelledby="cp-h">
          <h3 id="cp-h" class="colony-label mb-1">{t("safety.contentProtection")}</h3>
          <p class="colony-text-3xs mb-2 text-text-dim">{t("safety.contentProtectionDesc")}</p>
          <div class="flex items-center gap-2">
            <input
              id="sp-cp"
              type="checkbox"
              checked={config().content_protected}
              onChange={(e) => update({ content_protected: e.currentTarget.checked })}
              class="h-4 w-4"
            />
            <label for="sp-cp" class="colony-text-2xs text-text-secondary">
              {t("safety.contentProtection")}
            </label>
          </div>
        </section>

        {/* G5d — panic-tap threshold */}
        <section aria-labelledby="panic-tap-h">
          <h3 id="panic-tap-h" class="colony-label mb-1">Panic-tap threshold</h3>
          <p class="colony-text-3xs mb-2 text-text-dim">
            Rapid title-bar taps that trigger panic-wipe. 0 disables.
            Server bounds the value to [0, 10] so panic-wipe stays
            reachable in a real emergency.
          </p>
          <input
            id="sp-panic-taps"
            type="number"
            min="0"
            max="10"
            value={config().panic_tap_count}
            onInput={(e) => updatePanicTaps(e.currentTarget.value)}
            class="colony-text-2xs w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
          />
        </section>

        {/* Quick-hide (read-only display — registration of the
            global shortcut belongs to the platform shell, not this
            panel). */}
        <section aria-labelledby="qh-h">
          <h3 id="qh-h" class="colony-label mb-1">{t("safety.quickHide")}</h3>
          <p class="colony-text-3xs mb-2 text-text-dim">{t("safety.quickHideDesc")}</p>
          <p class="colony-text-2xs border border-bark bg-soil-deep px-2 py-1.5 text-text-secondary">
            {config().quick_hide_shortcut}
          </p>
        </section>

        <Show when={props.onOpenTravelMode}>
          <section aria-labelledby="travel-h">
            <h3 id="travel-h" class="colony-label mb-1">Travel mode</h3>
            <p class="colony-text-3xs mb-2 text-text-dim">
              Export an encrypted backup + wipe local data before a border
              crossing or device handover. Restore from backup later.
            </p>
            <button
              onClick={props.onOpenTravelMode}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep"
            >
              Open travel mode…
            </button>
          </section>
        </Show>

        <div class="flex gap-2 pt-2">
          <button onClick={save}
                  class="colony-text-2xs border-2 border-status-ok bg-soil-light px-3 py-1.5 text-status-ok hover:bg-soil-deep">
            {saved() ? t("common.saved") : t("safety.saveSafety")}
          </button>

          <Show when={props.onPanicWipe}>
            <button
              onClick={() => { void props.onPanicWipe?.(); }}
              class="colony-text-2xs ml-auto border-2 border-status-error bg-soil-light px-3 py-1.5 text-status-error hover:bg-soil-deep"
            >
              {t("safety.panicWipeButton")}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
};
