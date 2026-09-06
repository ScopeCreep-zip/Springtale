/**
 * Canvas overlays (ALIGNMENT-PLAN 3.6 — "overlays and an alert stack").
 *
 * Oxygen Not Included's overlays recolour the whole map by one variable and
 * grey out everything the variable does not describe. Ours do the same to the
 * springtails: one mode, one field, one colour scale. An agent the field says
 * nothing about (no formation under `momentum`) is dimmed rather than given a
 * made-up colour — the overlay never invents a reading.
 *
 * Pure functions. No reactivity, no component code.
 */
import type { ColonyAgent, ColonyFormation } from "./types";

/** The four modes, in cycle order. `none` is the normal canvas. */
export type OverlayMode = "none" | "momentum" | "attention" | "fuel";

export const OVERLAY_MODES: readonly OverlayMode[] = [
  "none",
  "momentum",
  "attention",
  "fuel",
] as const;

/** `O` cycles: none → momentum → attention → fuel → none. */
export function nextOverlay(mode: OverlayMode): OverlayMode {
  const i = OVERLAY_MODES.indexOf(mode);
  return OVERLAY_MODES[(i + 1) % OVERLAY_MODES.length] ?? "none";
}

/** Short label for the overlay chip; empty while no overlay is up. */
export function overlayLabel(mode: OverlayMode): string {
  return mode === "none" ? "" : mode.toUpperCase();
}

/**
 * The colour this agent reads as under `mode`, or `undefined` when the field
 * has nothing to say about it (then the canvas dims it).
 *
 * `momentum` is a formation property, so a solo agent has no reading;
 * `attention` and `fuel` are per-agent and always do.
 */
export function overlayColor(
  mode: OverlayMode,
  agent: ColonyAgent,
  formations: ColonyFormation[],
): string | undefined {
  switch (mode) {
    case "momentum": {
      if (!agent.connectorId) return undefined;
      const id = agent.connectorId;
      return formations.find((f) => f.members.includes(id))?.color;
    }
    case "attention":
      // The same thresholds the attention-overload dot uses.
      if (agent.attentionLoad > 0.7) return "var(--color-status-error)";
      if (agent.attentionLoad > 0.35) return "var(--color-status-warn)";
      return "var(--color-status-ok)";
    case "fuel":
      if (agent.fuelStatus === "critical") return "var(--color-status-error)";
      if (agent.fuelStatus === "warn") return "var(--color-status-warn)";
      return "var(--color-status-ok)";
    default:
      return undefined;
  }
}
