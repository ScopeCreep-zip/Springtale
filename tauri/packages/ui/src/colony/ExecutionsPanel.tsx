/**
 * Phase B — Executions log panel.
 *
 * Mounted in the BottomPanel detail view when an agent / formation
 * sprite is selected. Reads via `db.provider.listExecutions` (no
 * raw `invoke()` per the thin-frontend rule); fetches per-step
 * details on click. Sizes-only display matches the backend's
 * privacy posture — "Content not retained" placeholder when the
 * blob refs are null.
 *
 * Layout: timeline of recent runs (newest first), each row shows
 * status, mode, momentum, duration, trigger summary. Selecting a
 * row reveals its step list below.
 */

import type { Component } from "solid-js";
import { createResource, createSignal, For, Show } from "solid-js";

import { useDashboard } from "../dashboard/context";
import type {
  ExecutionFilterInput,
  ExecutionInfo,
  ExecutionStatusTag,
  ExecutionStepInfo,
} from "../dashboard/types";

export interface ExecutionsPanelProps {
  /** Filter the panel to this agent's runs. */
  botId?: string;
  /** Filter the panel to this formation's runs. */
  formationId?: string;
  /** Filter the panel to one rule's runs. */
  ruleId?: string;
  /** Cap on rows fetched (default 20 — the panel is a compact strip). */
  limit?: number;
}

const STATUS_LABEL: Record<ExecutionStatusTag, string> = {
  running: "⏳ Running",
  succeeded: "✓ Succeeded",
  empty: "○ Empty",
  failed: "✗ Failed",
  aborted: "⊘ Aborted",
  timed_out: "⏱ Timed out",
};

const STATUS_CLASS: Record<ExecutionStatusTag, string> = {
  running: "text-text-secondary",
  succeeded: "text-status-ok",
  empty: "text-text-dim",
  failed: "text-status-warn",
  aborted: "text-status-warn",
  timed_out: "text-status-warn",
};

// `status` is a plain string in the generated contract, not the UI's
// narrowed union: a run whose status the frontend does not know about
// must still render rather than crash.
function statusClass(status: string): string {
  return STATUS_CLASS[status as ExecutionStatusTag] ?? "text-text-dim";
}

function statusLabel(status: string): string {
  return STATUS_LABEL[status as ExecutionStatusTag] ?? status;
}

export const ExecutionsPanel: Component<ExecutionsPanelProps> = (props) => {
  const db = useDashboard();

  const filterKey = () =>
    JSON.stringify({
      bot_id: props.botId,
      formation_id: props.formationId,
      rule_id: props.ruleId,
      limit: props.limit ?? 20,
    });

  const [runs, { refetch }] = createResource(filterKey, async () => {
    const filter: ExecutionFilterInput = {
      bot_id: props.botId,
      formation_id: props.formationId,
      rule_id: props.ruleId,
      limit: props.limit ?? 20,
    };
    return db.provider.listExecutions(filter);
  });

  const [selectedRunId, setSelectedRunId] = createSignal<string | null>(null);
  const [steps, { refetch: refetchSteps }] = createResource(selectedRunId, async (id) => {
    if (!id) return [] as ExecutionStepInfo[];
    return db.provider.getExecutionSteps(id);
  });

  // Reload list whenever the underlying filter changes.
  const _ = filterKey; // referenced for reactivity

  return (
    <div class="rounded border border-bark bg-soil-mid">
      <header class="flex items-center justify-between border-b border-bark px-3 py-2">
        <p class="colony-text-sm font-bold text-text-primary">Executions</p>
        <button
          type="button"
          class="colony-text-3xs rounded border border-bark px-2 py-1 hover:bg-soil-deep"
          onClick={() => {
            refetch();
            refetchSteps();
          }}
        >
          Refresh
        </button>
      </header>

      <Show when={runs.loading}>
        <p class="colony-text-xs px-3 py-2 text-text-dim">Loading…</p>
      </Show>

      <Show when={!runs.loading && (runs()?.length ?? 0) === 0}>
        <p class="colony-text-xs px-3 py-3 text-text-dim">No runs recorded yet.</p>
      </Show>

      <ul class="max-h-[40vh] overflow-y-auto">
        <For each={runs() ?? []}>
          {(run) => (
            <li>
              <RunRow
                run={run}
                selected={selectedRunId() === run.id}
                onSelect={() => setSelectedRunId(run.id)}
              />
            </li>
          )}
        </For>
      </ul>

      <Show when={selectedRunId()}>
        <div class="border-t border-bark bg-soil-deep p-3">
          <p class="colony-text-xs font-bold text-text-primary">Steps</p>
          <Show when={steps.loading}>
            <p class="colony-text-3xs mt-1 text-text-dim">Loading steps…</p>
          </Show>
          <Show when={!steps.loading && (steps()?.length ?? 0) === 0}>
            <p class="colony-text-3xs mt-1 text-text-dim">No steps recorded for this run.</p>
          </Show>
          <ul class="mt-1">
            <For each={steps() ?? []}>{(step) => <StepRow step={step} />}</For>
          </ul>
        </div>
      </Show>
    </div>
  );
};

const RunRow: Component<{
  run: ExecutionInfo;
  selected: boolean;
  onSelect: () => void;
}> = (props) => {
  return (
    <button
      type="button"
      class="w-full border-b border-bark px-3 py-2 text-left hover:bg-soil-deep"
      classList={{ "bg-soil-deep": props.selected }}
      onClick={props.onSelect}
    >
      <div class="flex items-center justify-between">
        <span class={`colony-text-xs ${statusClass(props.run.status)}`}>
          {statusLabel(props.run.status)}
        </span>
        <span class="colony-text-3xs text-text-dim">
          {props.run.duration_ms != null ? `${props.run.duration_ms}ms` : "—"}
        </span>
      </div>
      <div class="colony-text-3xs mt-0.5 text-text-secondary">
        {props.run.trigger_summary ?? props.run.mode}
        <Show when={props.run.momentum}>
          <span class="ml-2 text-text-dim">· {props.run.momentum}</span>
        </Show>
      </div>
      <div class="colony-text-3xs text-text-dim">
        {formatTimestamp(props.run.started_at)}
        <Show when={props.run.error_kind}>
          <span class="ml-2 text-status-warn">· {props.run.error_kind}</span>
        </Show>
      </div>
    </button>
  );
};

const StepRow: Component<{ step: ExecutionStepInfo }> = (props) => {
  const duration = () =>
    props.step.finished_at != null ? props.step.finished_at - props.step.started_at : null;
  return (
    <li class="colony-text-3xs mt-1 rounded border border-bark px-2 py-1">
      <div class="flex justify-between">
        <span class="text-text-primary">
          #{props.step.step_index} {props.step.step_kind}
          <Show when={props.step.connector}>
            <span class="ml-1 text-text-secondary">
              → {props.step.connector}.{props.step.action ?? "-"}
            </span>
          </Show>
        </span>
        <span class="text-text-dim">
          {props.step.status} · {duration() ?? "?"}ms
        </span>
      </div>
      <div class="text-text-dim">
        out: {humanBytes(props.step.output_bytes)} ({props.step.output_kind ?? "—"})
        <Show
          when={props.step.output_blob_ref == null}
          fallback={<span class="ml-2 text-text-secondary">· content retained</span>}
        >
          <span class="ml-2">· content not retained</span>
        </Show>
      </div>
      <Show when={props.step.error_kind}>
        <div class="text-status-warn">err: {props.step.error_kind}</div>
      </Show>
    </li>
  );
};

function formatTimestamp(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "";
  return new Date(ms).toLocaleString();
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}KB`;
  return `${(n / 1024 / 1024).toFixed(1)}MB`;
}
