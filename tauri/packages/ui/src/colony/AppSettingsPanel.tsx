import { createSignal, Show } from "solid-js";
import type { Component } from "solid-js";
import { useI18n } from "../i18n/context";

export interface AppSettingsPanelProps {
  /** Open vault lock/unlock dialog */
  onVault: () => void;
  /** Trigger panic wipe (with confirmation) */
  onPanicWipe: () => void;
  /** Export all data */
  onExportData: () => Promise<void>;
  /** Compact bot memory */
  onCompactMemory: () => Promise<void>;
  /** Close the panel */
  onClose: () => void;
  /** Whether this is desktop (shows vault/panic) or web (hides them) */
  isDesktop: boolean;
  /** Current theme name */
  theme?: string;
  /** Called when user changes theme */
  onThemeChange?: (theme: string) => void;
}

/**
 * App-level settings panel.
 *
 * Scoped to THIS instance of Springtale — not per-bot settings.
 * Per product model: vault, safety, language, data export.
 */
export const AppSettingsPanel: Component<AppSettingsPanelProps> = (props) => {
  const { t, locale, setLocale } = useI18n();
  const [exporting, setExporting] = createSignal(false);
  const [compacting, setCompacting] = createSignal(false);
  const [error, setError] = createSignal("");

  return (
    <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
      <div class="mb-4 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">Settings</h2>
        <button onClick={props.onClose} class="colony-close-btn">✕</button>
      </div>

      {error() && (
        <div class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error">
          {error()}
        </div>
      )}

      <div class="space-y-4">
        {/* Security — desktop only */}
        <Show when={props.isDesktop}>
          <section>
            <h3 class="colony-label mb-2">SECURITY</h3>
            <div class="space-y-2">
              <button onClick={props.onVault}
                class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep">
                Vault (Lock / Unlock)
              </button>
              <button onClick={props.onPanicWipe}
                class="colony-text-2xs w-full border-2 border-status-error bg-soil-light px-3 py-1.5 text-status-error hover:bg-soil-deep">
                Emergency Wipe
              </button>
            </div>
            <div class="mt-2 rounded border border-bark-light bg-soil-deep p-2">
              <p class="colony-text-3xs text-text-dim">
                If you think your device might be monitored, consider using a
                different device to set up Springtale. For tech safety resources
                visit techsafety.org
              </p>
            </div>
          </section>
        </Show>

        {/* Theme */}
        <Show when={props.onThemeChange}>
          <section>
            <h3 class="colony-label mb-2">THEME</h3>
            <select
              value={props.theme ?? "colony"}
              onChange={(e) => props.onThemeChange?.(e.currentTarget.value)}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
            >
              <option value="springtale">Springtale (default)</option>
              <option value="colony">Colony</option>
            </select>
          </section>
        </Show>

        {/* Language */}
        <section>
          <h3 class="colony-label mb-2">LANGUAGE</h3>
          <select
            value={locale()}
            onChange={(e) => setLocale(e.currentTarget.value as "en")}
            class="colony-text-2xs w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
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
        </section>

        {/* Data */}
        <section>
          <h3 class="colony-label mb-2">DATA</h3>
          <div class="space-y-2">
            <button
              onClick={async () => {
                setExporting(true);
                try { await props.onExportData(); } catch (e) { setError(String(e)); }
                setExporting(false);
              }}
              disabled={exporting()}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep disabled:opacity-50"
            >
              {exporting() ? "Exporting..." : "Export All Data"}
            </button>
            <button
              onClick={async () => {
                setCompacting(true);
                try { await props.onCompactMemory(); } catch (e) { setError(String(e)); }
                setCompacting(false);
              }}
              disabled={compacting()}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep disabled:opacity-50"
            >
              {compacting() ? "Compacting..." : "Compact Bot Memory"}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
};
