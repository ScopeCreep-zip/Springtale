/**
 * Phase B — JSON Schema editor for AI structured extraction.
 *
 * Renders a textarea-style editor with backend-validated submit
 * (the runtime's preflight check rejects invalid schemas at deploy
 * time; this surface previews the parse error inline so the user
 * fixes it before clicking Deploy). Per `feedback_zero_frontend_logic`,
 * we don't validate against the JSON Schema spec here — we only do
 * "is the text valid JSON" sanity. The recipe-author's "this
 * adapter doesn't support structured outputs" check happens in the
 * preflight panel.
 *
 * Phase C upgrades this to a Monaco-style editor with field-name
 * autocomplete and a "test extract" button that runs the schema
 * against sample text via a dry-run backend op.
 */
import { Show, createSignal, createEffect } from "solid-js";
import type { Component } from "solid-js";

export interface AiSchemaEditorProps {
  /** Current schema value as a JSON object. May be null when unset. */
  value: unknown;
  /** Called with the parsed schema object whenever the textarea
   *  becomes a valid JSON object. */
  onChange: (next: unknown) => void;
  /** Optional example payload to render as a non-editable hint
   *  next to the editor. */
  example?: unknown;
}

export const AiSchemaEditor: Component<AiSchemaEditorProps> = (props) => {
  const initial = () => {
    try {
      return JSON.stringify(props.value ?? defaultSchema(), null, 2);
    } catch {
      return JSON.stringify(defaultSchema(), null, 2);
    }
  };

  const [text, setText] = createSignal(initial());
  const [error, setError] = createSignal<string | null>(null);

  // Keep textarea in sync if `props.value` changes underneath us
  // (e.g. recipe re-applied externally).
  createEffect(() => {
    const next = initial();
    if (next !== text()) {
      setText(next);
    }
  });

  const onInput = (ev: InputEvent & { currentTarget: HTMLTextAreaElement }) => {
    const next = ev.currentTarget.value;
    setText(next);
    try {
      const parsed = JSON.parse(next);
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
        setError("Schema must be a JSON object.");
        return;
      }
      setError(null);
      props.onChange(parsed);
    } catch (e) {
      setError(`Invalid JSON: ${(e as Error).message}`);
    }
  };

  return (
    <div class="rounded border border-bark bg-soil-mid">
      <header class="border-b border-bark px-3 py-2">
        <p class="colony-text-sm font-bold text-text-primary">
          AI extraction schema
        </p>
        <p class="colony-text-3xs mt-1 text-text-dim">
          JSON Schema (draft 2020-12). Top-level properties become the
          extracted fields. Requires an AI provider that supports
          structured outputs (OpenAI gpt-4o-2024-08-06+, Claude Sonnet
          4+, or Ollama 0.5+).
        </p>
      </header>
      <textarea
        class="colony-scrollbar w-full resize-y bg-soil-deep p-3 colony-text-xs text-text-primary font-mono max-h-64 overflow-y-auto"
        rows="10"
        value={text()}
        onInput={onInput}
        spellcheck={false}
      />
      <Show when={error()}>
        <p class="colony-text-3xs border-t border-bark px-3 py-1 text-status-warn">
          {error()}
        </p>
      </Show>
      <Show when={props.example}>
        <details class="border-t border-bark">
          <summary class="cursor-pointer px-3 py-2 colony-text-3xs text-text-secondary">
            Example output
          </summary>
          <pre class="overflow-x-auto bg-soil-deep p-3 colony-text-3xs text-text-dim">
            {JSON.stringify(props.example, null, 2)}
          </pre>
        </details>
      </Show>
    </div>
  );
};

/**
 * Sensible empty starting point — an object with one string field.
 * Recipe authors customize from here. Keeping the default
 * concrete (rather than `{}`) means new users get something that
 * already passes the preflight schema check.
 */
function defaultSchema(): Record<string, unknown> {
  return {
    type: "object",
    properties: {
      title: { type: "string" },
    },
    required: ["title"],
  };
}
