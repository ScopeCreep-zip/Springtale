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
  /** Plan 3.5 — the connector this agent's rule *acts on* (`AgentState.action_connector`).
   *  While the agent is firing the canvas walks it part-way along the mycelium
   *  toward this tree; `null` means it stays home. */
  actionConnectorId: string | null;
  task: string;
  status: "ok" | "warn" | "idle" | "error";
  pipeline: string | null;
  /** Backend's fetch-time `compute_activity`; seeds `activityOf` only until the agent's first utterance. */
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
  /** Plan 3.3 — the reports tab: this bot's execution history. */
  | { mode: "reports"; ruleId: string }
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
}

// ── Command definitions per context ─────────────────────

const cmd = (
  icon: string,
  label: string,
  key: string,
  context: ColonyCommand["context"],
  action: string,
): ColonyCommand => ({ icon, label, key, context, action });

/**
 * The command card — one fixed 3×3 grid per selection context.
 *
 * Plan 3.3: a StarCraft command card is usable because the same kind of
 * verb always sits in the same cell, and the destructive verb sits apart.
 * Slot meaning is therefore identical across contexts, read row-major:
 *
 * ```
 *   activate  |  suspend  |  step up      ← row 1: primary state changes
 *   configure |  inspect  |  step down    ← row 2: inspection
 *   group     |  move     |  DESTRUCTIVE  ← row 3: composition
 * ```
 *
 * The bottom-right cell is the only destructive verb in any grid and always
 * runs behind the confirm dialog (`controller.ts`). `null` is a spacer, not
 * a button: a context with no verb for a slot leaves the cell empty so every
 * other verb keeps its position. No paging — the whole card is always shown.
 *
 * Every action here has exactly one `case` in the controller's `handleCommand`
 * switch; `scripts/check-command-verbs.mjs` fails the lint if that stops
 * being true.
 */
export const COMMANDS: Record<string, (ColonyCommand | null)[]> = {
  none: [
    // Home card: no entity is selected, so row 1 is the three everyday
    // destinations, row 2 inspection, row 3 composition. Nothing global is
    // destructive, so the bottom-right cell stays empty.
    cmd("?", "ASK", "A", "global", "global:chat"),
    cmd("*", "BOTS", "B", "global", "global:bots"),
    cmd("^", "CONNECTORS", "C", "global", "global:connectors"),
    cmd("%", "SETTINGS", "S", "global", "global:settings"),
    cmd(".", "EVENTS", "E", "global", "global:events"),
    null,
    cmd("+", "MAKE BOT", "M", "global", "global:make_bot"),
    cmd("+", "NEW RULE", "R", "global", "global:new_rule"),
    null,
  ],
  agent: [
    cmd(">>", "RESUME", "M", "agent", "agent:resume"),
    cmd("||", "PAUSE", "P", "agent", "agent:pause"),
    cmd("^", "AUTO +", "=", "agent", "agent:autonomy_up"),
    cmd("@", "AI", "A", "agent", "agent:ai_config"),
    cmd("?", "INSPECT", "I", "agent", "agent:inspect"),
    cmd("v", "AUTO -", "-", "agent", "agent:autonomy_down"),
    cmd("[]", "GROUP", "G", "agent", "agent:group"),
    cmd("<>", "REASSIGN", "S", "agent", "agent:reassign"),
    cmd("x", "DETACH", "D", "agent", "agent:detach"),
  ],
  connector: [
    cmd("+", "ENABLE", "E", "connector", "connector:enable"),
    cmd("x", "DISABLE", "D", "connector", "connector:disable"),
    cmd(">", "TEST", "T", "connector", "connector:test"),
    cmd("%", "CONFIG", "C", "connector", "connector:config"),
    cmd(".", "EVENTS", "V", "connector", "connector:events"),
    // "O" cycles the canvas overlay (plan 3.6) and Shift+O opens the
    // canvas/OUTPUT view, so OUTPUTS binds "U".
    cmd("^", "OUTPUTS", "U", "connector", "connector:outputs"),
    null,
    null,
    cmd("-", "REMOVE", "R", "connector", "connector:remove"),
  ],
  // F1: formation context is backend-supplied via
  // `provider.formationAvailableCommands(id)` (B11). The status-aware
  // enable/disable + canonical hotkeys live entirely in Rust
  // (`crates/springtale-runtime/src/operations/commands.rs`); no
  // hardcoded fallback here.
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
/** Role → glyph in "Springtale Symbols" (the shipped Nerd Font subset), for
 *  the `yield` mote: the icon of the teammate you are thinking of. Codepoints
 *  mirror `utterance/defs.rs` and are checked by `springtale cooperation glyphs`. */
export const ROLE_GLYPHS: Record<string, string> = {
  scout: "\u{f00a5}", // nf-md-binoculars
  worker: "\u{f08ea}", // nf-md-hammer
  guard: "\u{f0498}", // nf-md-shield
  analyst: "\u{f0349}", // nf-md-magnify
  sentinel: "\u{f0208}", // nf-md-eye
};
export const ROLE_COLORS: Record<string, string> = {
  scout: "var(--color-role-scout)",
  worker: "var(--color-role-worker)",
  guard: "var(--color-role-guard)",
  analyst: "var(--color-role-analyst)",
  sentinel: "var(--color-role-sentinel)",
};

/** The four levels the backend actually has (`AutonomyLevel`): observe →
 *  suggest → approve → autonomous. No fifth level exists. */
export const AUTONOMY_LABELS = ["OBSERVE", "SUGGEST", "APPROVE", "AUTONOMOUS"];
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
