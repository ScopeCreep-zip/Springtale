/**
 * RecipeDeployPanel — W1.C focused deploy form.
 *
 * Progressive-disclosure pattern (Apple Print Dialog, iOS Settings):
 *   - Required fields are always visible at the top.
 *   - "Show more options" reveals optional fields.
 *   - "Show advanced" reveals trigger conditions / scheduling /
 *     advanced controls.
 *   - "Show as code" renders the assembled TOML so power users see
 *     what's actually being deployed. Round-trip TOML editing is a
 *     W2 enhancement; for W1.C this layer is read-only.
 *
 * Architecture invariant: every field shape (label, kind, default,
 * hint, required-vs-optional-vs-advanced classification) comes from
 * the `Recipe` payload. This component renders the fields it's told
 * to render and dispatches one call (`applyRecipe`) on Deploy. The
 * TOML rendering is a backend call (`renderRecipeToml`) so the
 * substitution logic stays in Rust and is shared across surfaces.
 */

import type { Component } from "solid-js";
import {
  createEffect,
  createMemo,
  createResource,
  createSignal,
  For,
  onCleanup,
  Show,
  Suspense,
} from "solid-js";
import { DisclosureSection } from "../DisclosureSection";
import { useDashboard } from "../dashboard/context";
import type {
  FieldKind,
  InputField,
  PreflightFix,
  PreflightReport,
  PreviewReport,
  Recipe,
  RecipeApplyReport,
  RecipeInputs,
} from "../dashboard/types";
import { AiSchemaEditor } from "./AiSchemaEditor";
import { CronFrequencyChip } from "./CronFrequencyChip";
import { PreflightChecklist } from "./PreflightChecklist";
import { PreviewPanel } from "./PreviewPanel";
import { WorkspaceTargetPicker } from "./WorkspaceTargetPicker";

export interface RecipeDeployPanelProps {
  recipe: Recipe;
  /** Formation the recipe will deploy into. Threaded through to
   *  WorkspaceTarget pickers so their dropdowns scope to this
   *  formation's mental_model_workspaces (Directory Facilitator
   *  extension — see `docs/intended-arch/COOPERATION.md §21`). */
  formationId?: string;
  /** Fires after a successful deploy. */
  onDeployed: (report: RecipeApplyReport) => void;
  /** Dismiss without deploying. */
  onCancel: () => void;
}

/** Build the initial inputs map from each field's `default`. */
function initialInputs(recipe: Recipe): RecipeInputs {
  const values: Record<string, unknown> = {};
  for (const f of recipe.inputs) {
    if (f.default !== undefined && f.default !== null) {
      values[f.id] = f.default;
    }
  }
  return { values };
}

/** Filter the recipe's inputs by author-declared visibility. */
function inputsWith(recipe: Recipe, visibility: InputField["visibility"]): InputField[] {
  return recipe.inputs.filter((f) => f.visibility === visibility);
}

export const RecipeDeployPanel: Component<RecipeDeployPanelProps> = (props) => {
  const db = useDashboard();
  const [inputs, setInputs] = createSignal<RecipeInputs>(initialInputs(props.recipe));
  const [deploying, setDeploying] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // W1.D — Preflight state. Debounced 500ms so quick typing doesn't
  // hammer the backend; runs immediately on first mount so the user
  // sees the checklist before they've edited anything.
  const [preflight, setPreflight] = createSignal<PreflightReport | null>(null);
  const [preflightLoading, setPreflightLoading] = createSignal(false);
  let preflightTimer: ReturnType<typeof setTimeout> | undefined;

  // W2.C — Preview state. Only fetched when the user clicks Preview.
  const [preview, setPreview] = createSignal<PreviewReport | null>(null);
  const [previewLoading, setPreviewLoading] = createSignal(false);
  // W1.C — Show-as-code disclosure open state. The TOML render fires
  // only when the disclosure is open so we don't pay backend
  // round-trips for users who never look at it.
  const [codeOpen, setCodeOpen] = createSignal(false);

  const setValue = (id: string, value: unknown) => {
    setInputs((prev) => ({
      values: { ...prev.values, [id]: value },
    }));
  };

  const runPreflight = async () => {
    setPreflightLoading(true);
    try {
      const report = await db.provider.preflightRecipe(props.recipe.id, inputs());
      setPreflight(report);
    } catch (e) {
      setError(`Preflight failed: ${String(e)}`);
    } finally {
      setPreflightLoading(false);
    }
  };

  // Re-run preflight 500ms after the user stops editing.
  createEffect(() => {
    void inputs(); // dependency tracking
    if (preflightTimer) clearTimeout(preflightTimer);
    preflightTimer = setTimeout(() => {
      void runPreflight();
    }, 500);
  });

  onCleanup(() => {
    if (preflightTimer) clearTimeout(preflightTimer);
  });

  const handleFix = (fix: PreflightFix) => {
    if (fix.kind === "focus_input") {
      const el = document.querySelector<HTMLInputElement>(`[data-input-id="${fix.input_id}"]`);
      el?.focus();
    }
    // Other fix kinds (open_ai_config / open_connector_config) need
    // parent-level dispatching; W1.D leaves them as visual hints
    // until the parent app wires their target panels into a callback.
  };

  const handlePreview = async () => {
    setPreviewLoading(true);
    try {
      const report = await db.provider.previewRecipe(props.recipe.id, inputs());
      setPreview(report);
    } catch (e) {
      setError(`Preview failed: ${String(e)}`);
    } finally {
      setPreviewLoading(false);
    }
  };

  const handleDeploy = async () => {
    setDeploying(true);
    setError(null);
    try {
      const report = await db.provider.applyRecipe(props.recipe.id, inputs());
      // Re-fetch rules/connectors so the newly-deployed bot's tree/agent
      // sprite appears on the live colony canvas immediately — the canvas
      // projects sprites from the rules/connectors lists, so without this
      // the new bot only shows up after a manual refresh. Every other
      // mutation handler in `dashboard/context.ts` refreshes the same way.
      await db.refresh();
      props.onDeployed(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setDeploying(false);
    }
  };

  const deployBlocked = createMemo(() => {
    const p = preflight();
    if (!p) return true; // haven't probed yet — be conservative
    return !p.deployable;
  });

  // Show-as-code is gated on the user actually opening the disclosure
  // so we don't pay the backend round-trip for everyone on every keystroke.
  const codeKey = createMemo(() => (codeOpen() ? JSON.stringify(inputs()) : null));
  const [code] = createResource(codeKey, async (key) => {
    if (!key) return "";
    return db.provider.renderRecipeToml(props.recipe.id, inputs());
  });

  return (
    <div class="mx-auto flex w-full max-w-3xl flex-col rounded border-2 border-bark bg-soil-mid max-h-[calc(100vh-4rem)]">
      {/* ── Sticky header ──────────────────────────────────────── */}
      <div class="flex items-start justify-between border-b border-bark p-4">
        <div>
          <p class="colony-text-md font-bold text-text-primary">USE: {props.recipe.name}</p>
          <p class="colony-text-xs mt-1 text-text-secondary">{props.recipe.description}</p>
        </div>
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-3 py-1"
          onClick={() => props.onCancel()}
        >
          Cancel
        </button>
      </div>

      {/* ── Scrollable body ───────────────────────────────────── */}
      <div class="colony-scrollbar flex-1 overflow-y-auto p-6">
        <Show when={inputsWith(props.recipe, "required").length > 0}>
          <div class="space-y-3">
            <For each={inputsWith(props.recipe, "required")}>
              {(f) => (
                <FieldInput
                  field={f}
                  required
                  value={inputs().values[f.id]}
                  onChange={(v) => setValue(f.id, v)}
                  inputs={inputs().values}
                  formationId={props.formationId}
                />
              )}
            </For>
          </div>
        </Show>

        <Show when={inputsWith(props.recipe, "optional").length > 0}>
          <div class="mt-4">
            <DisclosureSection
              title="Show more options"
              hint={`${inputsWith(props.recipe, "optional").length} optional`}
            >
              <div class="mt-2 space-y-3">
                <For each={inputsWith(props.recipe, "optional")}>
                  {(f) => (
                    <FieldInput
                      field={f}
                      value={inputs().values[f.id]}
                      onChange={(v) => setValue(f.id, v)}
                      inputs={inputs().values}
                    />
                  )}
                </For>
              </div>
            </DisclosureSection>
          </div>
        </Show>

        <Show when={inputsWith(props.recipe, "advanced").length > 0}>
          <div class="mt-2">
            <DisclosureSection
              title="Show advanced"
              hint={`${inputsWith(props.recipe, "advanced").length} fields`}
            >
              <div class="mt-2 space-y-3">
                <For each={inputsWith(props.recipe, "advanced")}>
                  {(f) => (
                    <FieldInput
                      field={f}
                      value={inputs().values[f.id]}
                      onChange={(v) => setValue(f.id, v)}
                      inputs={inputs().values}
                    />
                  )}
                </For>
              </div>
            </DisclosureSection>
          </div>
        </Show>

        <div class="mt-2">
          <DisclosureSection
            title="Show as code"
            hint="Read-only preview of the assembled config"
            onToggle={(open) => setCodeOpen(open)}
          >
            <Suspense fallback={<p class="colony-text-3xs text-text-dim">Rendering…</p>}>
              <pre class="colony-text-3xs mt-2 max-h-72 overflow-auto rounded border border-bark bg-soil-deep p-3 text-text-primary">
                {code() ?? ""}
              </pre>
            </Suspense>
          </DisclosureSection>
        </div>

        <div class="mt-4">
          <PreflightChecklist report={preflight()} loading={preflightLoading()} onFix={handleFix} />
        </div>

        <Show when={preview() || previewLoading()}>
          <div class="mt-3">
            <PreviewPanel
              report={preview()}
              loading={previewLoading()}
              onClose={() => setPreview(null)}
            />
          </div>
        </Show>

        <Show when={error()}>
          <p class="colony-text-2xs mt-2 text-status-error">{error()}</p>
        </Show>
      </div>

      {/* ── Sticky footer ─────────────────────────────────────── */}
      <div class="flex justify-end gap-2 border-t border-bark bg-soil-mid p-4">
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-5 py-2"
          disabled={previewLoading()}
          onClick={handlePreview}
        >
          {previewLoading() ? "Previewing…" : "Preview"}
        </button>
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-5 py-2"
          style={{ "border-color": "var(--color-status-ok)" }}
          disabled={deploying() || deployBlocked()}
          onClick={handleDeploy}
        >
          {deploying() ? "Deploying…" : "Deploy"}
        </button>
      </div>
    </div>
  );
};

// ── Field input — renders one InputField row ─────────────────────

interface FieldInputProps {
  field: InputField;
  required?: boolean;
  value: unknown;
  onChange: (value: unknown) => void;
  /** Sibling inputs by id — forwarded to FieldControl for the
   *  CssSelector renderer's `sample_url` resolution. */
  inputs?: Record<string, unknown>;
  /** Formation id — forwarded to FieldControl so WorkspaceTarget
   *  fields can scope their dropdown to this formation's
   *  mental_model_workspaces. */
  formationId?: string;
}

const FieldInput: Component<FieldInputProps> = (props) => {
  return (
    <fieldset>
      <legend class="colony-text-2xs flex items-baseline gap-2 text-text-secondary">
        <span>{props.field.label}</span>
        <Show when={props.required}>
          <span class="colony-text-3xs text-status-warn">required</span>
        </Show>
        <Show when={isSecret(props.field.kind)}>
          <span class="colony-text-3xs text-text-dim">🔒 secret</span>
        </Show>
      </legend>
      <FieldControl
        field={props.field}
        value={props.value}
        onChange={props.onChange}
        inputs={props.inputs}
        formationId={props.formationId}
      />
      <Show when={props.field.hint}>
        <p class="colony-text-3xs mt-1 text-text-dim">{props.field.hint}</p>
      </Show>
    </fieldset>
  );
};

interface FieldControlProps {
  field: InputField;
  value: unknown;
  onChange: (value: unknown) => void;
  /**
   * Sibling inputs by id. Used by the CssSelector renderer to
   * resolve `sample_url` references — `kind.sample_url: "url"`
   * means "open the picker against the value of the `url` input".
   * Optional so the existing call sites that don't need cross-field
   * lookups keep working.
   */
  inputs?: Record<string, unknown>;
  /**
   * Formation the deploy will land in. WorkspaceTarget fields use
   * this to scope the `mental_model_workspaces` dropdown and to
   * build the Telegram onboard deep-link payload.
   */
  formationId?: string;
}

function isSecret(kind: FieldKind): boolean {
  return kind.kind === "secret";
}

const FieldControl: Component<FieldControlProps> = (props) => {
  const kind = () => props.field.kind;
  const valueStr = () => {
    const v = props.value;
    if (v === undefined || v === null) return "";
    if (typeof v === "string") return v;
    return String(v);
  };

  if (kind().kind === "bool") {
    return (
      <label class="colony-text-xs mt-1 flex items-center gap-2 text-text-primary">
        <input
          type="checkbox"
          data-input-id={props.field.id}
          checked={Boolean(props.value)}
          onChange={(e) => props.onChange(e.currentTarget.checked)}
        />
        <span>{props.value ? "On" : "Off"}</span>
      </label>
    );
  }

  if (kind().kind === "number") {
    return (
      <input
        type="number"
        data-input-id={props.field.id}
        class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
        value={valueStr()}
        onInput={(e) => {
          const raw = e.currentTarget.value;
          if (raw === "") {
            props.onChange(null);
          } else {
            const n = Number(raw);
            props.onChange(Number.isNaN(n) ? raw : n);
          }
        }}
      />
    );
  }

  if (kind().kind === "select") {
    const k = kind();
    if (k.kind !== "select") return null;
    return (
      <select
        data-input-id={props.field.id}
        class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
        value={valueStr()}
        onChange={(e) => props.onChange(e.currentTarget.value)}
      >
        <For each={k.options}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
      </select>
    );
  }

  // Phase A.5 — Cron expression + frequency chip mirroring the
  // backend `ScheduleBucket`. The chip is visual hint; preflight
  // remains authoritative.
  if (kind().kind === "cron") {
    return (
      <div class="mt-1">
        <div class="flex items-center">
          <input
            type="text"
            data-input-id={props.field.id}
            class="colony-text-xs flex-1 border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
            value={valueStr()}
            placeholder="0 7 * * *"
            onInput={(e) => props.onChange(e.currentTarget.value)}
          />
          <CronFrequencyChip expression={valueStr()} />
        </div>
      </div>
    );
  }

  // Phase B.6 — CSS selector picker. Text input + 🎯 Pick button
  // that opens the desktop webview overlay; the picker resolves
  // the sample URL by reading the sibling input named in
  // `kind.sample_url`. Web users fall through to the overlay's
  // graceful "type a selector manually" message.
  if (kind().kind === "css_selector") {
    const k = kind();
    if (k.kind !== "css_selector") return null;
    const db = useDashboard();
    const [pickerError, setPickerError] = createSignal<string | null>(null);
    const [pickerWorking, setPickerWorking] = createSignal(false);
    const sampleUrl = () => {
      if (!k.sample_url) return "";
      const sibling = props.inputs?.[k.sample_url];
      return typeof sibling === "string" ? sibling : "";
    };
    const openPicker = async () => {
      setPickerError(null);
      setPickerWorking(true);
      try {
        const picked = await db.provider.openSelectorPicker(sampleUrl(), []);
        if (picked === null) {
          // User cancelled, or the platform (web) doesn't support
          // the native picker. Surface a hint either way — either's
          // a valid reason to fall back to typing the selector
          // manually or copying from DevTools.
          setPickerError(
            "Picker closed without a selection. Type a selector here, or use your browser's DevTools (right-click → Inspect → Copy → Selector).",
          );
          return;
        }
        props.onChange(picked);
      } catch (e) {
        setPickerError(String(e));
      } finally {
        setPickerWorking(false);
      }
    };
    return (
      <div class="mt-1">
        <div class="flex items-center gap-2">
          <input
            type="text"
            data-input-id={props.field.id}
            class="colony-text-xs flex-1 border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
            value={valueStr()}
            placeholder="main, h1.title, [data-id]"
            onInput={(e) => props.onChange(e.currentTarget.value)}
          />
          <button
            type="button"
            class="colony-text-3xs rounded border border-bark bg-soil-mid px-2 py-1 hover:bg-soil-light disabled:opacity-50"
            disabled={!sampleUrl() || pickerWorking()}
            title={sampleUrl() ? `Open picker at ${sampleUrl()}` : "Set the URL field first"}
            onClick={openPicker}
          >
            {pickerWorking() ? "…" : "🎯 Pick"}
          </button>
        </div>
        <Show when={pickerError()}>
          <p class="colony-text-3xs mt-1 text-status-warn">{pickerError()}</p>
        </Show>
      </div>
    );
  }

  // Phase B.6 — JSON Schema editor (AI structured extraction).
  // Defers to AiSchemaEditor for the actual JSON editor; we just
  // wire its value <-> onChange to the recipe input.
  if (kind().kind === "json_schema") {
    const k = kind();
    if (k.kind !== "json_schema") return null;
    return (
      <div class="mt-1">
        <AiSchemaEditor value={props.value} example={k.example} onChange={props.onChange} />
      </div>
    );
  }

  // D1 — Workspace target. Dropdown over the formation's
  // mental_model_workspaces filtered by connector, with scan +
  // onboard + manual entry affordances. `formInputs` flows the
  // deploy form's credentials (bot_token etc.) down so the
  // connector-agnostic Onboard button can resolve a deep link
  // without the connector being registered yet.
  if (kind().kind === "workspace_target") {
    const k = kind();
    if (k.kind !== "workspace_target") return null;
    return (
      <WorkspaceTargetPicker
        connector={k.connector}
        kinds={k.kinds}
        formationId={props.formationId ?? ""}
        value={valueStr()}
        onChange={props.onChange}
        formInputs={props.inputs}
      />
    );
  }

  const isPassword = kind().kind === "secret";
  return (
    <input
      type={isPassword ? "password" : "text"}
      data-input-id={props.field.id}
      class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
      value={valueStr()}
      onInput={(e) => props.onChange(e.currentTarget.value)}
    />
  );
};
