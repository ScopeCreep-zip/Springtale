/**
 * Phase C — drift badge for a recipe row.
 *
 * Queries `db.provider.getRecipeDrift` on mount, renders one of:
 *   - nothing (not_enough_data — hide rather than confuse).
 *   - "steady" chip (neutral grey).
 *   - "improving" chip (green, ▲).
 *   - "degrading" chip (yellow, ▼).
 *
 * The classification is server-side per
 * `feedback_zero_frontend_logic` — we never re-derive it from the
 * raw numbers, we just render what the backend told us.
 *
 * The badge tooltip surfaces median latency + p95 + success / refusal
 * percentages from the report so power users can see what's
 * actually changing.
 */

import type { Component } from "solid-js";
import { createResource, Show } from "solid-js";

import { useDashboard } from "../dashboard/context";
import type { DriftClass, DriftReport } from "../dashboard/types";

export interface DriftBadgeProps {
  recipeId: string;
  botId?: string;
  formationId?: string;
}

const LABEL: Record<DriftClass, string> = {
  not_enough_data: "",
  steady: "◇ steady",
  improving: "▲ improving",
  degrading: "▼ degrading",
};

const CLASS_FOR: Record<DriftClass, string> = {
  not_enough_data: "hidden",
  steady: "border-bark bg-soil-deep text-text-dim",
  improving: "border-status-ok bg-soil-deep text-status-ok",
  degrading: "border-status-warn bg-soil-deep text-status-warn",
};

export const DriftBadge: Component<DriftBadgeProps> = (props) => {
  const db = useDashboard();
  const [report] = createResource(
    () => ({
      recipeId: props.recipeId,
      botId: props.botId,
      formationId: props.formationId,
    }),
    (key) =>
      db.provider.getRecipeDrift(key.recipeId, {
        bot_id: key.botId,
        formation_id: key.formationId,
      }),
  );

  return (
    <Show when={report() && report()?.overall !== "not_enough_data"} keyed>
      {(_) => {
        const r = report();
        if (!r) return null;
        return (
          <span
            class={`colony-text-3xs rounded border px-1 ${CLASS_FOR[r.overall]}`}
            title={tooltip(r)}
          >
            {LABEL[r.overall]}
          </span>
        );
      }}
    </Show>
  );
};

function tooltip(r: DriftReport): string {
  const parts: string[] = [];
  parts.push(`runs: ${r.recent_runs} recent vs ${r.baseline_runs} baseline`);
  if (r.latency.recent_median_ms != null) {
    parts.push(
      `median latency: ${r.latency.recent_median_ms}ms (was ${r.latency.baseline_median_ms ?? "—"}ms)`,
    );
  }
  if (r.success_rate.recent != null) {
    parts.push(
      `success: ${pct(r.success_rate.recent)} (was ${pct(r.success_rate.baseline ?? null)})`,
    );
  }
  if (r.refusal_rate.recent != null) {
    parts.push(
      `refusal: ${pct(r.refusal_rate.recent)} (was ${pct(r.refusal_rate.baseline ?? null)})`,
    );
  }
  return parts.join(" · ");
}

function pct(value: number | null): string {
  if (value == null) return "—";
  return `${(value * 100).toFixed(0)}%`;
}
