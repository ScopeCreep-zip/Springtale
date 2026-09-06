import type { Component } from "solid-js";
import { createResource } from "solid-js";
import { useDashboard } from "../../dashboard/context";

/**
 * Renders the currently-bound AI adapter for this formation (read from
 * `ai:formation:{id}` config key). Clicking dispatches
 * `formation:ai_adapter` so the host App can open the shared
 * `AiConfigPanel` scoped to the formation. Adapter-resolution precedence
 * (agent → formation → global) lives in `resolve_ai_config` server-side
 * per the thin-frontend rule.
 */
export const FormationAiAdapterRow: Component<{
  formationId: string;
  onCommand: (action: string) => void;
}> = (props) => {
  const db = useDashboard();
  const [config] = createResource(
    () => props.formationId,
    async (id) => {
      try {
        return await db.provider.getConfig(`ai:formation:${id}`);
      } catch {
        return null;
      }
    },
  );

  const label = () => {
    const c = config();
    if (!c || (typeof c === "object" && c !== null && Object.keys(c).length === 0)) {
      return "inherit";
    }
    const t = (c as { type?: string }).type;
    return t ? t.toUpperCase() : "inherit";
  };

  return (
    <>
      <div class="colony-label mt-1.5 mb-0.5">AI ADAPTER</div>
      <button
        type="button"
        class="colony-text-2xs flex w-full items-center justify-between rounded border border-bark bg-soil-deep px-2 py-1 hover:border-bark-light"
        onClick={() => props.onCommand("formation:ai_adapter")}
      >
        <span class="text-text-secondary">{label()}</span>
        <span class="colony-text-xs text-text-dim">click to override</span>
      </button>
    </>
  );
};

// ── Command Grid ─────────────────────────────────────────
