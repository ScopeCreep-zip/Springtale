/**
 * Phase A.5 — Cron frequency classification chip.
 *
 * Renders next to a `FieldKind::Cron` input. Classifies the current
 * value into one of the backend's ScheduleBucket states and shows a
 * colour-coded chip:
 *
 *   - sub-minute → 🚫 Blocking ("rate-limit hits + races")
 *   - 1–4 min   → ⚠️ Warning ("most sites consider abusive")
 *   - ≥ 5 min   → ✓ Standard
 *   - 15 min+   → ✓ Preferred (Zapier free-tier default)
 *
 * The classification is *the same set of thresholds* the backend
 * `check_schedule_frequency` uses — per `feedback_zero_frontend_logic`
 * the frontend mirrors the rule, never invents new thresholds.
 * Backend preflight remains authoritative; this chip is purely a
 * visual hint while the user types.
 */
import { Show } from "solid-js";
import type { Component } from "solid-js";

export interface CronFrequencyChipProps {
  /** Current cron expression value. */
  expression: string;
}

type Bucket = "invalid" | "sub_minute" | "aggressive" | "standard" | "preferred";

const LABEL: Record<Bucket, string> = {
  invalid: "✗ invalid",
  sub_minute: "🚫 sub-minute",
  aggressive: "⚠️ aggressive",
  standard: "✓ standard",
  preferred: "✓ preferred",
};

const CHIP_CLASS: Record<Bucket, string> = {
  invalid: "border-status-warn text-status-warn",
  sub_minute: "border-status-warn text-status-warn",
  aggressive: "border-status-warn text-status-warn",
  standard: "border-bark text-text-secondary",
  preferred: "border-status-ok text-status-ok",
};

const TOOLTIP: Record<Bucket, string> = {
  invalid: "Not a valid 5- or 6-field cron expression.",
  sub_minute:
    "Sub-minute schedules cause source-site rate-limit hits + race conditions. Raise to ≥1 minute. Backend will block deploy.",
  aggressive:
    "1–4 minute polling. Most sites consider sub-5-minute polling abusive. Backend will warn at deploy.",
  standard: "Standard polling cadence (≥5 minutes).",
  preferred: "Preferred polling cadence (≥15 minutes, Zapier free-tier default).",
};

export const CronFrequencyChip: Component<CronFrequencyChipProps> = (props) => {
  const bucket = () => classify(props.expression);
  return (
    <Show when={props.expression.trim().length > 0}>
      <span
        class={`colony-text-3xs ml-2 inline-block rounded border bg-soil-deep px-1 ${CHIP_CLASS[bucket()]}`}
        title={TOOLTIP[bucket()]}
      >
        {LABEL[bucket()]}
      </span>
    </Show>
  );
};

/**
 * Mirror of the backend's `ScheduleBucket::from_cron` in
 * `crates/springtale-runtime/src/operations/preflight/checks.rs`.
 * Keep these in sync — if the backend's thresholds change, this
 * function updates too.
 */
function classify(expr: string): Bucket {
  const fields = expr.trim().split(/\s+/);
  if (fields.length === 6) {
    const sec = fields[0] ?? "";
    if (sec === "*") return "sub_minute";
    const m = /^\*\/(\d+)$/.exec(sec);
    if (m && m[1] !== undefined) {
      const n = parseInt(m[1], 10);
      if (Number.isFinite(n) && n < 60) return "sub_minute";
    }
    return classifyMinuteField(fields[1] ?? "");
  }
  if (fields.length === 5) {
    return classifyMinuteField(fields[0] ?? "");
  }
  return "invalid";
}

function classifyMinuteField(field: string): Bucket {
  const m = /^\*\/(\d+)$/.exec(field);
  if (m && m[1] !== undefined) {
    const n = parseInt(m[1], 10);
    if (!Number.isFinite(n) || n === 0) return "invalid";
    if (n < 5) return "aggressive";
    if (n >= 15) return "preferred";
    return "standard";
  }
  if (field === "*") return "aggressive";
  // Fixed values, ranges, lists → at-worst once-per-minute over the
  // matching minutes. Treat fixed hourly/daily as preferred when
  // the field is non-numeric (rare in cron); other fixed values
  // are standard.
  return "standard";
}
