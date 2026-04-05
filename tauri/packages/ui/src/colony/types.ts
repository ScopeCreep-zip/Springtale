/**
 * Colony visual model types.
 *
 * These map from DashboardState (abstract data) to the colony
 * ecosystem visualization (spatial, visual, game-inspired).
 */

export interface ColonyTree {
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
  fuel: number;
  hp: number;
  treeId: string | null;
  task: string;
  status: "ok" | "warn" | "idle" | "error";
  pipeline: string | null;
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
}

export interface ColonySelection {
  id: string | null;
  type: "tree" | "agent" | "formation" | null;
}

export interface ColonyCommand {
  icon: string;
  label: string;
  key: string;
}

export const TREE_TYPES = ["conifer", "deciduous", "shrub"] as const;
export const TREE_SPRITES: Record<string, string> = {
  conifer: "sprite-tree-conifer",
  deciduous: "sprite-tree-deciduous",
  shrub: "sprite-tree-shrub",
};
export const TREE_SIZES: Record<string, { width: number; height: number }> = {
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

export const MUSHROOM_SPRITES = ["sprite-mushroom-gold", "sprite-mushroom-purple", "sprite-mushroom-teal"];

export const COMMANDS: Record<string, (ColonyCommand | null)[]> = {
  none: [
    { icon: "*", label: "INSTINCTS", key: "I" },
    { icon: "~", label: "CANVAS", key: "V" },
    { icon: "^", label: "TREES", key: "T" },
    { icon: "-", label: "MEMORY", key: "M" },
    { icon: "@", label: "AI", key: "F" },
    { icon: "%", label: "SETTINGS", key: "S" },
    null, null, null,
  ],
  agent: [
    { icon: "<>", label: "REASSIGN", key: "R" },
    { icon: "^", label: "AUTO +", key: "=" },
    { icon: "v", label: "AUTO -", key: "-" },
    { icon: "||", label: "PAUSE", key: "P" },
    { icon: "<<", label: "RECALL", key: "C" },
    { icon: "x", label: "DETACH", key: "D" },
    { icon: "?", label: "INSPECT", key: "I" },
    { icon: "[]", label: "GROUP", key: "G" },
    null,
  ],
  tree: [
    { icon: "+", label: "ENABLE", key: "E" },
    { icon: "x", label: "DISABLE", key: "D" },
    { icon: "%", label: "CONFIG", key: "C" },
    { icon: "-", label: "REMOVE", key: "R" },
    { icon: ".", label: "EVENTS", key: "V" },
    { icon: ">", label: "TEST", key: "T" },
    { icon: "^", label: "OUTPUTS", key: "O" },
    null, null,
  ],
  formation: [
    { icon: ">", label: "INTENT", key: "I" },
    { icon: "+", label: "ADD", key: "A" },
    { icon: "~", label: "FUEL", key: "F" },
    { icon: "#", label: "GUARD", key: "G" },
    { icon: "x", label: "DISSOLVE", key: "D" },
    { icon: "!", label: "RALLY", key: "R" },
    null, null, null,
  ],
};

/** Deterministic hash for stable layout positions */
export function hash(str: string): number {
  let h = 5381;
  for (let i = 0; i < str.length; i++) {
    h = ((h << 5) + h) + str.charCodeAt(i);
  }
  return h;
}

export function seeded(key: string, min: number, max: number): number {
  return min + ((hash(key) & 0x7FFFFFFF) % (max - min));
}
