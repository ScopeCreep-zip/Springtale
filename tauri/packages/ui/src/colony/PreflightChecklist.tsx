/**
 * PreflightChecklist — W1.D Deploy-readiness rendering.
 *
 * Pure renderer. Every classification (blocking / warning / verified /
 * pending) comes from the backend `PreflightReport`. Frontend only:
 *   - Renders the rows with appropriate colour + glyph.
 *   - Surfaces "Fix this" buttons for `PreflightFix` hints by
 *     forwarding the click to the parent (which knows which panel
 *     to open).
 *
 * Per `feedback_preflight_zero_to_live`: the worst UX is a bot that
 * deploys and silently doesn't work. This component makes the
 * "missing X" state visible before the user clicks Deploy.
 */

import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type {
  PreflightFix,
  PreflightItem,
  PreflightReport,
  PreflightStatus,
} from "../dashboard/types";

export interface PreflightChecklistProps {
  /** `null` = haven't probed yet (form just opened). */
  report: PreflightReport | null;
  /** `true` while a fresh report is in flight; renders a subtle
   *  pulse so the user knows their last edit is being validated. */
  loading?: boolean;
  /** Called when the user clicks a "Fix this" button on a row. The
   *  parent dispatches to the appropriate surface (focus an input,
   *  open AI config, open connector setup). */
  onFix?: (fix: PreflightFix) => void;
}

export const PreflightChecklist: Component<PreflightChecklistProps> = (props) => {
  return (
    <div class="rounded border-2 border-bark bg-soil-deep p-3">
      <div class="flex items-center justify-between">
        <p class="colony-text-2xs font-bold text-text-primary">Ready to deploy?</p>
        <Show when={props.loading}>
          <span class="colony-text-3xs text-text-dim">checking…</span>
        </Show>
      </div>

      <Show when={!props.report && !props.loading}>
        <p class="colony-text-3xs mt-2 text-text-dim">
          Fill in the required fields above to see the checklist.
        </p>
      </Show>

      <Show when={props.report}>
        <ul class="mt-2 space-y-1">
          <For each={props.report!.items}>
            {(item) => (
              <PreflightRow item={item} onFix={props.onFix} />
            )}
          </For>
        </ul>
        <Show when={!props.report!.deployable}>
          <p class="colony-text-3xs mt-3 text-status-error">
            Resolve the items above to enable Deploy.
          </p>
        </Show>
        <Show when={props.report!.deployable && props.report!.has_warnings}>
          <p class="colony-text-3xs mt-3 text-status-warn">
            Deploy is allowed; warnings flagged above will be applied at deploy time.
          </p>
        </Show>
      </Show>
    </div>
  );
};

interface PreflightRowProps {
  item: PreflightItem;
  onFix?: (fix: PreflightFix) => void;
}

const PreflightRow: Component<PreflightRowProps> = (props) => {
  return (
    <li class="colony-text-3xs flex items-start gap-2">
      <span class="mt-px">{statusGlyph(props.item.status)}</span>
      <div class="flex-1">
        <div class="text-text-primary">{props.item.label}</div>
        <Show when={props.item.detail}>
          <div class={statusDetailClass(props.item.status)}>
            {props.item.detail}
          </div>
        </Show>
      </div>
      <Show when={props.item.fix_hint && props.onFix}>
        <button
          class="colony-text-3xs rounded border border-bark px-2 py-0.5 text-text-primary hover:bg-soil-mid"
          onClick={() => props.onFix?.(props.item.fix_hint!)}
        >
          {fixLabel(props.item.fix_hint!)}
        </button>
      </Show>
    </li>
  );
};

function statusGlyph(status: PreflightStatus): string {
  switch (status) {
    case "blocking":
      return "🔴";
    case "warning":
      return "🟡";
    case "verified":
      return "🟢";
    case "pending":
      return "⏳";
  }
}

function statusDetailClass(status: PreflightStatus): string {
  switch (status) {
    case "blocking":
      return "text-status-error";
    case "warning":
      return "text-status-warn";
    case "verified":
      return "text-text-dim";
    case "pending":
      return "text-text-dim";
  }
}

function fixLabel(fix: PreflightFix): string {
  switch (fix.kind) {
    case "focus_input":
      return "Fix";
    case "open_ai_config":
      return "Configure AI";
    case "open_connector_config":
      return "Set up";
    case "note":
      return "Info";
  }
}
