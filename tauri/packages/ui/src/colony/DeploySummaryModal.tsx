/**
 * Phase C — post-deploy summary modal (Apify "your actor is
 * ready" pattern).
 *
 * Renders the `ApplyReport` the backend returned from
 * `apply_recipe`: which bots / rules / connectors got set up, and
 * the recipe-supplied summary. No business logic — the report is
 * authoritative per the thin-frontend rule.
 */

import type { Component } from "solid-js";
import { For, Show } from "solid-js";

import type { RecipeApplyReport } from "../dashboard/types";

export interface DeploySummaryModalProps {
  report: RecipeApplyReport;
  /** Called when the user dismisses the modal. */
  onClose: () => void;
  /** Optional — wired by the parent to switch to the executions
   *  panel for the newly-created rule(s). */
  onViewExecutions?: () => void;
}

export const DeploySummaryModal: Component<DeploySummaryModalProps> = (props) => {
  return (
    <div class="mx-auto max-w-xl rounded border-2 border-bark bg-soil-mid p-6">
      <p class="colony-text-md font-bold text-text-primary">✓ Deployed</p>
      <p class="colony-text-xs mt-2 text-text-secondary">{props.report.summary}</p>

      <dl class="mt-4 space-y-2">
        <Show when={props.report.connectors_configured.length > 0}>
          <div>
            <dt class="colony-text-xs font-bold text-text-primary">Connectors configured</dt>
            <dd class="colony-text-3xs text-text-secondary">
              <For each={props.report.connectors_configured}>
                {(name) => <span class="mr-2">{name}</span>}
              </For>
            </dd>
          </div>
        </Show>

        <Show when={props.report.rules_created.length > 0}>
          <div>
            <dt class="colony-text-xs font-bold text-text-primary">Rules created</dt>
            <dd class="colony-text-3xs text-text-secondary">
              {props.report.rules_created.length} rule
              {props.report.rules_created.length === 1 ? "" : "s"}.
              <For each={props.report.rules_created}>
                {(id) => <span class="ml-2 text-text-dim font-mono">{id}</span>}
              </For>
            </dd>
          </div>
        </Show>

        <Show when={props.report.ai_configured}>
          <div>
            <dt class="colony-text-xs font-bold text-text-primary">AI</dt>
            <dd class="colony-text-3xs text-text-secondary">Provider configured by this recipe.</dd>
          </div>
        </Show>
      </dl>

      <div class="mt-6 flex justify-end gap-2">
        <Show when={props.onViewExecutions}>
          <button
            type="button"
            class="colony-text-sm rounded border border-bark px-4 py-2 hover:bg-soil-deep"
            onClick={props.onViewExecutions}
          >
            View runs
          </button>
        </Show>
        <button
          type="button"
          class="colony-text-sm rounded border border-bark bg-soil-deep px-4 py-2 text-text-primary hover:bg-soil-light"
          onClick={props.onClose}
        >
          Done
        </button>
      </div>
    </div>
  );
};
