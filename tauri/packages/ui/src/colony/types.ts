/**
 * Colony visual model types.
 *
 * These map from DashboardState (abstract data) to the colony
 * network visualization (spatial, visual, game-inspired diorama).
 */

export interface ColonyNode {
  id: string;
  label: string;
  type: "conifer" | "deciduous" | "shrub";
  x: number;
  y: number;
  status: "active" | "paused" | "idle";
}

export interface ColonyAgent {
  id: string;
  name: string;
  role: "scout" | "worker" | "guard" | "analyst" | "sentinel";
  autonomy: number;
  /** Human label from backend: "OBSERVE" | "SUGGEST" | "APPROVE" | "AUTONOMOUS". */
  autonomyLabel: string;
  fuel: number;
  /** Fuel status from backend threshold: "ok" | "warn" | "critical". */
  fuelStatus: "ok" | "warn" | "critical";
  hp: number;
  connectorId: string | null;
  task: string;
  status: "ok" | "warn" | "idle" | "error";
  pipeline: string | null;
  /** Activity state from backend: "firing" | "error" | "active" | "waiting" | "idle". */
  activity?: string;
  /** Attention load from cooperation layer (0.0-1.0). */
  attentionLoad: number;
  /** Liveness score (1.0 = alive, 0.0 = dead). */
  liveness: number;
  /** Health state: "healthy" | "degraded" | "incapacitated" | "dead". */
  healthState: string;
}

export interface ColonyPipe {
  id: string;
  dir: 1 | -1;
  status: "active" | "warning" | "idle";
}

export interface ColonyConnection {
  a: string;
  b: string;
  pipes: ColonyPipe[];
}

export interface ColonyFormation {
  id: string;
  name: string;
  intent: string;
  description: string;
  momentum: number;
  momentumLabel: string;
  color: string;
  members: string[];
  zone: { x: number; y: number };
  status: string;
  rallyTokens: number;
  rallyMax: number;
  /** Backend guard badge label ("GUARD" when engaged); absent until the backend reports it. */
  guardStatus?: string;
  /**
   * Phase H6 — current pacing phase derived from the most recent
   * `pacing_phase_changed` cooperation event for this formation. Drives a
   * single CSS class on the formation sprite (`data-pacing="..."`).
   * Possible values match the backend `PacingPhase` enum (snake_case
   * serialized as: "preparation", "active", "peak", "recovery",
   * "disruption"). Stored slugged for direct CSS-attribute use.
   */
  pacingPhase?: "prep" | "active" | "peak" | "recovery" | "disrupted";
  /**
   * W7 — current cascade streak, present only while this formation is
   * *actively* cascading. Derived from the most-recent `cascade_hit`
   * cooperation event, gated on recency against the colony's own latest
   * event timestamp (not wall-clock), so it reflects "cascading right now"
   * and clears on its own as the event timeline moves past it. `undefined`
   * = not cascading; never decoration.
   */
  cascadeStreak?: number;
}

export interface ColonySelection {
  id: string | null;
  type: "connector" | "agent" | "formation" | null;
}

/**
 * Detail panel view mode — what the bottom-center panel shows.
 *
 * RTS pattern: the detail panel is context-sensitive.
 * Entity detail when something is selected, list views when
 * a global command is pressed.
 */
export type DetailView =
  | { mode: "colony" }
  | { mode: "entity" }
  | { mode: "bots" }
  | { mode: "connectors" }
  | { mode: "events"; filterConnector?: string }
  | { mode: "outputs"; connectorId: string }
  | { mode: "formations"; addAgentId?: string }
  /** W2.E — A2UI canvas block surface; renders structured output from
   *  the bot's `CanvasState` via the shared `Canvas` component. */
  | { mode: "canvas" };

/**
 * Colony command — context-aware, type-safe.
 *
 * Each command carries its context (what type of entity is selected)
 * so the handler can dispatch unambiguously. No flat string matching.
 *
 * Inspired by StarCraft/C&C command card pattern:
 * different unit types → different command grids → different handlers.
 */
export interface ColonyCommand {
  /** Display icon (pixel art char) */
  icon: string;
  /** Display label */
  label: string;
  /** Keyboard shortcut */
  key: string;
  /** Context this command belongs to — determines dispatch */
  context: "global" | "connector" | "agent" | "formation";
  /** Unique action ID for dispatch (context:action) */
  action: string;
  /**
   * W6 Nintendo 3-action rule: the command grid leads with the (≤3)
   * `primary` commands per context and tucks the rest behind a "MORE"
   * drawer, so a new user is never shown more than three choices at once.
   * Marked on the everyday actions; everything else is secondary.
   */
  primary?: boolean;
}

// ── Command definitions per context ─────────────────────

const cmd = (
  icon: string,
  label: string,
  key: string,
  context: ColonyCommand["context"],
  action: string,
  primary = false,
): ColonyCommand => ({ icon, label, key, context, action, primary });

export const COMMANDS: Record<string, (ColonyCommand | null)[]> = {
  none: [
    // Canvas is live (formations + connectors + cooperation events stream
    // in via subscriptions), so an explicit Refresh command is dead UI.
    // Slot reclaimed for MAKE BOT — the entry point back to the
    // bot/team selection hub (ModeSelectOverlay) for adding more bots
    // after the canvas already has some.
    // ASK is the primary chat entry point (W5) — talk to the bot directly
    // to get the weather, research, scrape, or make a change. First slot so
    // it's the home grid's lead action (Nintendo 3-action: Ask / Bots /
    // Connectors are the everyday three).
    // Primary three (Nintendo rule): Ask / Bots / Connectors — what a user
    // reaches for every day. The rest live behind MORE.
    cmd("?", "ASK", "A", "global", "global:chat", true),
    cmd("*", "BOTS", "B", "global", "global:bots", true),
    cmd("^", "CONNECTORS", "C", "global", "global:connectors", true),
    cmd("+", "MAKE BOT", "M", "global", "global:make_bot"),
    cmd("+", "NEW RULE", "N", "global", "global:new_rule"),
    cmd(".", "EVENTS", "E", "global", "global:events"),
    cmd("%", "SETTINGS", "S", "global", "global:settings"),
    null,
    null,
  ],
  agent: [
    // Primary three: configure AI, dial autonomy up/down — the levers you
    // touch on a running agent. Pause/reassign/detach/etc. behind MORE.
    cmd("@", "AI", "A", "agent", "agent:ai_config", true),
    cmd("^", "AUTO +", "=", "agent", "agent:autonomy_up", true),
    cmd("v", "AUTO -", "-", "agent", "agent:autonomy_down", true),
    cmd("||", "PAUSE", "P", "agent", "agent:pause"),
    cmd("<>", "REASSIGN", "R", "agent", "agent:reassign"),
    cmd("x", "DETACH", "D", "agent", "agent:detach"),
    cmd("?", "INSPECT", "I", "agent", "agent:inspect"),
    cmd("[]", "GROUP", "G", "agent", "agent:group"),
    cmd("<<", "RECALL", "C", "agent", "agent:recall"),
  ],
  connector: [
    // Primary three: enable / config / test — the setup-and-verify loop.
    cmd("+", "ENABLE", "E", "connector", "connector:enable", true),
    cmd("%", "CONFIG", "C", "connector", "connector:config", true),
    cmd(">", "TEST", "T", "connector", "connector:test", true),
    cmd("x", "DISABLE", "D", "connector", "connector:disable"),
    cmd("-", "REMOVE", "R", "connector", "connector:remove"),
    cmd(".", "EVENTS", "V", "connector", "connector:events"),
    cmd("^", "OUTPUTS", "O", "connector", "connector:outputs"),
    null,
    null,
  ],
  // F1: formation context is backend-supplied via
  // `provider.formationAvailableCommands(id)` (B11). The status-aware
  // enable/disable + canonical hotkeys live entirely in Rust
  // (`crates/springtale-runtime/src/operations/commands.rs`); no
  // hardcoded fallback here. App.tsx uses the resource directly when
  // dispatching formation hotkeys; if the resource hasn't resolved yet
  // it short-circuits without firing.
};

export const NODE_TYPES = ["conifer", "deciduous", "shrub"] as const;
export const NODE_SPRITES: Record<string, string> = {
  conifer: "sprite-tree-conifer",
  deciduous: "sprite-tree-deciduous",
  shrub: "sprite-tree-shrub",
};
export const NODE_SIZES: Record<string, { width: number; height: number }> = {
  conifer: { width: 36, height: 52 },
  deciduous: { width: 36, height: 44 },
  shrub: { width: 28, height: 20 },
};

export const ROLE_SPRITES: Record<string, string> = {
  scout: "sprite-scout",
  worker: "sprite-worker",
  guard: "sprite-guard",
  analyst: "sprite-analyst",
  sentinel: "sprite-sentinel",
};
export const ROLE_COLORS: Record<string, string> = {
  scout: "var(--color-role-scout)",
  worker: "var(--color-role-worker)",
  guard: "var(--color-role-guard)",
  analyst: "var(--color-role-analyst)",
  sentinel: "var(--color-role-sentinel)",
};

export const AUTONOMY_LABELS = ["OBSERVE", "SUGGEST", "APPROVE", "AUTONOMOUS", "SELF-DIRECT"];
export const MOMENTUM_NAMES = ["COLD", "WARM", "HOT", "FEVER"];
export const MOMENTUM_COLORS = [
  "var(--color-momentum-cold)",
  "var(--color-momentum-warm)",
  "var(--color-momentum-hot)",
  "var(--color-momentum-fever)",
];
export const MOMENTUM_UNLOCKS = ["Read only", "Basic chains", "Write env + sync", "Consensus + AI"];

export const TIER_CAPABILITIES: Record<number, string[]> = {
  0: ["read env"],
  1: ["read env", "neighbors", "chain"],
  2: ["read env", "neighbors", "chain", "write env", "commit"],
  3: ["read env", "neighbors", "chain", "write env", "commit", "consensus", "AI", "recruit"],
};

export const MUSHROOM_SPRITES = [
  "sprite-mushroom-gold",
  "sprite-mushroom-purple",
  "sprite-mushroom-teal",
];

/** Deterministic hash for stable layout positions */
export function hash(str: string): number {
  let h = 5381;
  for (let i = 0; i < str.length; i++) {
    h = (h << 5) + h + str.charCodeAt(i);
  }
  return h;
}

export function seeded(key: string, min: number, max: number): number {
  return min + ((hash(key) & 0x7fffffff) % (max - min));
}
