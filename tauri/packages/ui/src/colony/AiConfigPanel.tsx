import { createSignal, Show, For } from "solid-js";
import type { Component } from "solid-js";

export interface AiConfigPanelProps {
  agentId: string;
  agentName: string;
  onSave: (agentId: string, config: Record<string, unknown>) => Promise<void>;
  onClose: () => void;
}

const ADAPTER_TYPES = [
  { value: "noop", label: "No AI", description: "Classical command routing only. No API calls." },
  { value: "ollama", label: "Ollama (Local)", description: "Local LLM via Ollama. Private — no data leaves your machine." },
  { value: "openai", label: "OpenAI Compatible", description: "GPT, Gemini, DeepSeek, OpenRouter, vLLM, llama.cpp server." },
  { value: "anthropic", label: "Anthropic", description: "Claude API with native tool use." },
] as const;

/**
 * Per-bot AI adapter configuration panel.
 *
 * AI is per-bot, not global. Each bot gets its own adapter config.
 * Default is NoopAdapter — the bot works without AI.
 */
export const AiConfigPanel: Component<AiConfigPanelProps> = (props) => {
  const [adapterType, setAdapterType] = createSignal("noop");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [model, setModel] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");

  const handleSave = async () => {
    setSaving(true);
    setError("");
    try {
      // Send raw fields — backend applies adapter-specific defaults
      const config: Record<string, unknown> = {
        type: adapterType(),
        base_url: baseUrl() || undefined,
        api_key: apiKey() || undefined,
        model: model() || undefined,
      };
      await props.onSave(props.agentId, config);
      props.onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const inputClass = "colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none";

  return (
    <div class="mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6" style={{ "max-height": "80vh" }}>
      <div class="mb-4 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">
          AI Adapter — {props.agentName}
        </h2>
        <button onClick={props.onClose} class="colony-close-btn">✕</button>
      </div>

      <p class="colony-text-3xs mb-4 text-text-dim">
        Choose an AI adapter for this bot. Default is No AI — the bot works
        entirely on classical command matching. AI adds freeform conversation
        and NL→Rule parsing.
      </p>

      {error() && (
        <div class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error">
          {error()}
        </div>
      )}

      {/* Adapter type selection */}
      <div class="mb-4 space-y-2">
        <For each={ADAPTER_TYPES}>
          {(adapter) => (
            <label
              class={`flex cursor-pointer items-start gap-3 rounded border-2 p-3 ${
                adapterType() === adapter.value
                  ? "border-status-ok bg-status-ok/5"
                  : "border-bark hover:border-bark-light"
              }`}
            >
              <input
                type="radio"
                name="adapter-type"
                value={adapter.value}
                checked={adapterType() === adapter.value}
                onChange={() => setAdapterType(adapter.value)}
                class="mt-0.5"
              />
              <div>
                <span class="colony-text-2xs font-bold text-text-primary">{adapter.label}</span>
                <p class="colony-text-3xs mt-0.5 text-text-dim">{adapter.description}</p>
              </div>
            </label>
          )}
        </For>
      </div>

      {/* Adapter-specific fields */}
      <Show when={adapterType() === "ollama"}>
        <div class="mb-4 space-y-3">
          <div>
            <label class="colony-text-2xs text-text-secondary">Ollama URL</label>
            <input type="text" value={baseUrl()} onInput={(e) => setBaseUrl(e.currentTarget.value)}
              placeholder="http://127.0.0.1:11434" class={inputClass} />
          </div>
          <div>
            <label class="colony-text-2xs text-text-secondary">Model</label>
            <input type="text" value={model()} onInput={(e) => setModel(e.currentTarget.value)}
              placeholder="llama3.2" class={inputClass} />
          </div>
        </div>
      </Show>

      <Show when={adapterType() === "openai"}>
        <div class="mb-4 space-y-3">
          <div>
            <label class="colony-text-2xs text-text-secondary">API Base URL</label>
            <input type="text" value={baseUrl()} onInput={(e) => setBaseUrl(e.currentTarget.value)}
              placeholder="https://api.openai.com" class={inputClass} />
          </div>
          <div>
            <label class="colony-text-2xs text-text-secondary">API Key</label>
            <input type="password" value={apiKey()} onInput={(e) => setApiKey(e.currentTarget.value)}
              placeholder="sk-..." class={inputClass} />
          </div>
          <div>
            <label class="colony-text-2xs text-text-secondary">Model</label>
            <input type="text" value={model()} onInput={(e) => setModel(e.currentTarget.value)}
              placeholder="gpt-4o" class={inputClass} />
          </div>
        </div>
      </Show>

      <Show when={adapterType() === "anthropic"}>
        <div class="mb-4 space-y-3">
          <div>
            <label class="colony-text-2xs text-text-secondary">API Key</label>
            <input type="password" value={apiKey()} onInput={(e) => setApiKey(e.currentTarget.value)}
              placeholder="sk-ant-..." class={inputClass} />
          </div>
          <div>
            <label class="colony-text-2xs text-text-secondary">Model</label>
            <input type="text" value={model()} onInput={(e) => setModel(e.currentTarget.value)}
              placeholder="claude-sonnet-4-20250514" class={inputClass} />
          </div>
          <div>
            <label class="colony-text-2xs text-text-secondary">Base URL</label>
            <input type="text" value={baseUrl()} onInput={(e) => setBaseUrl(e.currentTarget.value)}
              placeholder="https://api.anthropic.com" class={inputClass} />
          </div>
        </div>
      </Show>

      <div class="flex gap-3">
        <button onClick={handleSave} disabled={saving()}
          class="colony-text-2xs border-2 border-status-ok bg-soil-light px-3 py-1.5 text-status-ok hover:bg-soil-deep disabled:opacity-50">
          {saving() ? "Saving..." : "Save"}
        </button>
        <button onClick={props.onClose}
          class="colony-text-2xs border-2 border-bark bg-soil-light px-3 py-1.5 text-text-secondary hover:bg-soil-deep">
          Cancel
        </button>
      </div>
    </div>
  );
};
