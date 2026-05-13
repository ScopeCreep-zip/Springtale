/**
 * PreviewPanel — W2.C comic-strip narrative for a recipe dry-run.
 *
 * Renders the backend `PreviewReport` as speech-bubble steps. Every
 * step's `speaker` and `narrative` come straight from the backend
 * (which does the substitution + redaction); this component is a
 * pure renderer.
 *
 * Used in two places:
 *   - `RecipeDeployPanel`'s "Preview" button (let the user see what
 *     deploy will do before committing).
 *   - W2.B authoring "Clear Check" gate — recipe save is blocked
 *     until preview passes.
 */

import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { PreviewReport } from "../dashboard/types";

export interface PreviewPanelProps {
  report: PreviewReport | null;
  loading?: boolean;
  /** Optional dismiss button — omit when the panel is embedded
   *  inline (RecipeDeployPanel) and shown alongside the form. */
  onClose?: () => void;
}

export const PreviewPanel: Component<PreviewPanelProps> = (props) => {
  return (
    <div class="rounded border-2 border-bark bg-soil-deep p-3">
      <div class="flex items-center justify-between">
        <p class="colony-text-2xs font-bold text-text-primary">Preview — what this bot will do</p>
        <Show when={props.onClose}>
          <button
            class="colony-text-3xs text-text-dim hover:text-text-primary"
            onClick={() => props.onClose?.()}
          >
            ✕
          </button>
        </Show>
      </div>

      <Show when={props.loading}>
        <p class="colony-text-3xs mt-2 text-text-dim">Running…</p>
      </Show>

      <Show when={props.report && !props.loading}>
        <Show when={!props.report!.passed}>
          <div class="mt-2 rounded border border-status-error bg-status-error/10 p-2">
            <p class="colony-text-3xs text-status-error">Preview failed:</p>
            <ul class="colony-text-3xs mt-1 list-disc pl-4 text-status-error">
              <For each={props.report!.errors}>{(err) => <li>{err}</li>}</For>
            </ul>
          </div>
        </Show>
        <ul class="mt-2 space-y-2">
          <For each={props.report!.steps}>
            {(step) => (
              <li class="rounded border border-bark bg-soil-mid p-2">
                <div class="colony-text-3xs text-text-dim">{step.speaker}</div>
                <div class="colony-text-2xs mt-1 text-text-primary">{step.narrative}</div>
                <Show when={step.would_send_to}>
                  <div class="colony-text-3xs mt-1 text-text-dim">
                    → would send to <span class="text-text-primary">{step.would_send_to}</span>
                  </div>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};
