import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { useI18n } from "../i18n/context";

/**
 * Bot settings (plan 6.3) — persona, context window and the AI tool
 * allow-list. Mirrors `springtale_runtime::operations::bot_settings::BotSettings`.
 */
export interface BotSettingsValue {
  persona: { name: string; tone: string; prefix: string };
  context_window: number;
  tool_policy: {
    allow: string[];
    deny: string[];
    max_iterations?: number;
    writes_with_approval?: boolean;
  };
}

export interface AppSettingsPanelProps {
  /** Open vault lock/unlock dialog */
  onVault: () => void;
  /** Trigger panic wipe (with confirmation) */
  onPanicWipe: () => void;
  /**
   * G5d — open the SafetyPanel overlay (app disguise, auto-lock,
   * content protection, panic-tap threshold). Optional so older
   * embeddings keep working; when omitted the SAFETY button hides.
   */
  onOpenSafety?: () => void;
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
  /**
   * Plan 6.3 — current bot settings. `null` while loading; omit the prop
   * entirely (with `onSaveBotSettings`) to hide the Bot section.
   */
  botSettings?: BotSettingsValue | null;
  /**
   * Tool names (`connector__action`) the AI may be allowed to call,
   * built from the installed connectors' declared actions. The
   * allow-list is a checkbox set over exactly these — no free text, so a
   * typo can't silently leave the bot tool-less.
   */
  availableTools?: string[];
  /** Persist the edited bot settings. */
  onSaveBotSettings?: (settings: BotSettingsValue) => Promise<void>;
}

/**
 * App-level settings panel.
 *
 * Scoped to THIS instance of Springtale — not per-bot settings.
 * Per product model: vault, safety, language, data export.
 */
export const AppSettingsPanel: Component<AppSettingsPanelProps> = (props) => {
  const { locale, setLocale } = useI18n();
  const [exporting, setExporting] = createSignal(false);
  const [compacting, setCompacting] = createSignal(false);
  const [error, setError] = createSignal("");

  // Bot settings edits: `null` = untouched, so the stored value shows
  // through until the user actually changes that field.
  const [botName, setBotName] = createSignal<string | null>(null);
  const [botPrefix, setBotPrefix] = createSignal<string | null>(null);
  const [botWindow, setBotWindow] = createSignal<number | null>(null);
  const [botAllow, setBotAllow] = createSignal<string[] | null>(null);
  const [savingBot, setSavingBot] = createSignal(false);

  const currentName = () => botName() ?? props.botSettings?.persona.name ?? "";
  const currentPrefix = () => botPrefix() ?? props.botSettings?.persona.prefix ?? "/";
  const currentWindow = () => botWindow() ?? props.botSettings?.context_window ?? 50;
  const currentAllow = () => botAllow() ?? props.botSettings?.tool_policy.allow ?? [];

  const toggleTool = (tool: string) => {
    const next = currentAllow().includes(tool)
      ? currentAllow().filter((t) => t !== tool)
      : [...currentAllow(), tool];
    setBotAllow(next);
  };

  const saveBot = async () => {
    const save = props.onSaveBotSettings;
    const existing = props.botSettings;
    if (!save || !existing) return;
    setSavingBot(true);
    setError("");
    try {
      await save({
        persona: {
          name: currentName(),
          tone: existing.persona.tone,
          prefix: currentPrefix().slice(0, 1) || "/",
        },
        context_window: currentWindow(),
        tool_policy: { ...existing.tool_policy, allow: currentAllow() },
      });
      setBotName(null);
      setBotPrefix(null);
      setBotWindow(null);
      setBotAllow(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSavingBot(false);
    }
  };

  return (
    <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
      <div class="mb-4 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">Settings</h2>
        <button type="button" onClick={props.onClose} class="colony-close-btn">
          ✕
        </button>
      </div>

      {error() && (
        <div class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error">
          {error()}
        </div>
      )}

      <div class="space-y-4">
        {/* Bot — persona, context window, AI tool allow-list (plan 6.3) */}
        <Show when={props.botSettings && props.onSaveBotSettings}>
          <section>
            <h3 class="colony-text-xs mb-2 font-bold text-text-secondary">Bot</h3>
            <div class="space-y-2">
              <label class="colony-text-2xs block text-text-secondary">
                Name
                <input
                  type="text"
                  class="colony-input mt-1 w-full"
                  value={currentName()}
                  onInput={(e) => setBotName(e.currentTarget.value)}
                />
              </label>
              <label class="colony-text-2xs block text-text-secondary">
                Command prefix
                <input
                  type="text"
                  maxLength={1}
                  class="colony-input mt-1 w-16"
                  value={currentPrefix()}
                  onInput={(e) => setBotPrefix(e.currentTarget.value)}
                />
              </label>
              <label class="colony-text-2xs block text-text-secondary">
                Context window (messages remembered)
                <input
                  type="number"
                  min="1"
                  class="colony-input mt-1 w-24"
                  value={currentWindow()}
                  onInput={(e) => setBotWindow(Number(e.currentTarget.value) || 1)}
                />
              </label>
              <div class="colony-text-2xs text-text-secondary">
                AI tools
                <Show
                  when={(props.availableTools ?? []).length > 0}
                  fallback={
                    <p class="colony-text-2xs mt-1 text-text-muted">
                      No connector actions installed yet.
                    </p>
                  }
                >
                  <p class="colony-text-2xs mt-1 text-text-muted">
                    None checked = read-only actions only.
                  </p>
                  <div class="mt-1 max-h-40 space-y-1 overflow-y-auto border border-bark p-2">
                    <For each={props.availableTools ?? []}>
                      {(tool) => (
                        <label class="flex items-center gap-2 text-text-primary">
                          <input
                            type="checkbox"
                            checked={currentAllow().includes(tool)}
                            onChange={() => toggleTool(tool)}
                          />
                          {tool}
                        </label>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
              <button
                type="button"
                class="colony-btn"
                disabled={savingBot()}
                onClick={() => void saveBot()}
              >
                {savingBot() ? "SAVING…" : "SAVE BOT"}
              </button>
            </div>
          </section>
        </Show>

        {/* Security — desktop only */}
        <Show when={props.isDesktop}>
          <section>
            <h3 class="colony-label mb-2">SECURITY</h3>
            <div class="space-y-2">
              <button
                type="button"
                onClick={props.onVault}
                class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep"
              >
                Vault (Lock / Unlock)
              </button>
              <button
                type="button"
                onClick={props.onPanicWipe}
                class="colony-text-2xs w-full border-2 border-status-error bg-soil-light px-3 py-1.5 text-status-error hover:bg-soil-deep"
              >
                Emergency Wipe
              </button>
              <Show when={props.onOpenSafety}>
                <button
                  type="button"
                  onClick={props.onOpenSafety}
                  class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep"
                >
                  Safety &amp; Disguise…
                </button>
              </Show>
            </div>
            <div class="mt-2 rounded border border-bark-light bg-soil-deep p-2">
              <p class="colony-text-3xs text-text-dim">
                If you think your device might be monitored, consider using a different device to
                set up Springtale. For tech safety resources visit techsafety.org
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
              type="button"
              onClick={async () => {
                setExporting(true);
                try {
                  await props.onExportData();
                } catch (e) {
                  setError(String(e));
                }
                setExporting(false);
              }}
              disabled={exporting()}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-light px-3 py-1.5 text-text-primary hover:bg-soil-deep disabled:opacity-50"
            >
              {exporting() ? "Exporting..." : "Export All Data"}
            </button>
            <button
              type="button"
              onClick={async () => {
                setCompacting(true);
                try {
                  await props.onCompactMemory();
                } catch (e) {
                  setError(String(e));
                }
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
