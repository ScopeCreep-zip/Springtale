/**
 * Phase C — per-step "Test This Step" button.
 *
 * Renders alongside a step in the RecipeDeployPanel. On click,
 * calls `db.provider.testRecipeStep` with the recipe + filled
 * inputs + step coordinates; renders the resulting StepOutput
 * inline (success: kind, duration, output JSON; failure: the
 * error string). Side-effecting steps come back stubbed
 * (`dry_run: true`) — the UI calls that out so users know the
 * actual destination didn't get pinged.
 */

import type { Component } from "solid-js";
import { createSignal, Show } from "solid-js";

import { useDashboard } from "../dashboard/context";
import type { RecipeInputs, TestStepReport } from "../dashboard/types";

export interface TestStepButtonProps {
  recipeId: string;
  ruleIndex: number;
  stepIndex: number;
  inputs: RecipeInputs;
  /** Optional label override — defaults to "🧪 Test this step". */
  label?: string;
}

export const TestStepButton: Component<TestStepButtonProps> = (props) => {
  const db = useDashboard();
  const [working, setWorking] = createSignal(false);
  const [result, setResult] = createSignal<TestStepReport | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const run = async () => {
    setWorking(true);
    setError(null);
    setResult(null);
    try {
      const report = await db.provider.testRecipeStep(
        props.recipeId,
        props.inputs,
        props.ruleIndex,
        props.stepIndex,
      );
      setResult(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  };

  const isDryRun = () => {
    const step = result()?.step;
    if (!step) return false;
    try {
      const out = JSON.parse(step.output_json) as { dry_run?: boolean };
      return out.dry_run === true;
    } catch {
      return false;
    }
  };

  return (
    <div class="rounded border border-bark bg-soil-deep p-2">
      <div class="flex items-center justify-between">
        <span class="colony-text-3xs text-text-secondary">Step {props.stepIndex + 1}</span>
        <button
          type="button"
          class="colony-text-3xs rounded border border-bark bg-soil-mid px-2 py-1 hover:bg-soil-light"
          onClick={run}
          disabled={working()}
        >
          {working() ? "Running…" : (props.label ?? "🧪 Test this step")}
        </button>
      </div>

      <Show when={error()}>
        <p class="colony-text-3xs mt-2 text-status-warn">{error()}</p>
      </Show>

      <Show when={result()}>
        {(reportAccessor) => {
          const report = reportAccessor();
          return (
            <div class="mt-2">
              <Show
                when={report.ran && report.step}
                fallback={
                  <p class="colony-text-3xs text-status-warn">
                    {report.error ?? "Step did not run."}
                  </p>
                }
              >
                <Show when={report.step}>
                  {(stepAccessor) => {
                    const step = stepAccessor();
                    return (
                      <div>
                        <p class="colony-text-3xs text-text-secondary">
                          {step.kind} · {step.duration_ms}ms
                          <Show when={isDryRun()}>
                            <span class="ml-2 rounded border border-bark bg-soil-mid px-1 text-text-dim">
                              dry-run · destination not contacted
                            </span>
                          </Show>
                        </p>
                        <pre class="mt-1 max-h-48 overflow-y-auto rounded border border-bark bg-soil-mid p-2 colony-text-3xs text-text-primary">
                          {prettyJson(step.output_json)}
                        </pre>
                      </div>
                    );
                  }}
                </Show>
              </Show>
            </div>
          );
        }}
      </Show>
    </div>
  );
};

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
