import { createSignal, Show } from "solid-js";
import {
  ColonyShell,
  TopBar,
  Viewport,
  BottomPanel,
  HatchWizard,
  useDashboard,
  useI18n,
  seeded,
} from "@springtale/ui";
import type { ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection } from "@springtale/ui";
import { SettingsPage } from "./pages/Settings";
import { SessionsPage } from "./pages/Sessions";

/**
 * Springtale Web Dashboard — colony ecosystem view.
 *
 * Same visual as the desktop app. Settings/sessions render
 * in the viewport overlay when triggered from command grid.
 */
export const App = () => {
  const db = useDashboard();
  const { t } = useI18n();

  // ── Colony selection state ─────────────────────────────
  const [selection, setSelection] = createSignal<ColonySelection>({ id: null, type: null });
  const [showSettings, setShowSettings] = createSignal(false);
  const [showSessions, setShowSessions] = createSignal(false);
  const [connected, setConnected] = createSignal(false);

  const handleSettingsSaved = async () => {
    setConnected(true);
    db.resubscribe();
    await db.refresh();
  };

  // ── Data → Colony visual model ─────────────────────────
  const TREE_TYPES_LIST = ["conifer", "deciduous", "shrub"] as const;

  const trees = (): ColonyTree[] =>
    db.connectors().map((c) => ({
      id: c.name,
      label: c.name,
      type: TREE_TYPES_LIST[seeded(c.name + "type", 0, 3)],
      x: seeded(c.name + "x", 8, 92),
      y: seeded(c.name + "y", 15, 70),
      status: c.enabled ? "active" as const : "idle" as const,
    }));

  const agents = (): ColonyAgent[] =>
    db.rules().map((r) => ({
      id: r.id,
      name: r.name,
      role: (["scout", "worker", "guard", "analyst"] as const)[seeded(r.id + "role", 0, 4)],
      autonomy: seeded(r.id + "auto", 0, 5),
      fuel: 50 + seeded(r.id + "fuel", 0, 50),
      hp: 80 + seeded(r.id + "hp", 0, 20),
      treeId: r.connector ?? r.triggerType,
      task: r.status === "enabled" ? `Processing ${r.triggerType}` : "Idle — disabled",
      status: r.status === "enabled" ? "ok" as const : "idle" as const,
      pipeline: r.triggerType,
    }));

  const connections = (): ColonyConnection[] => {
    const t = trees();
    if (t.length < 2) return [];
    const conns: ColonyConnection[] = [];
    for (let i = 0; i < t.length - 1 && i < 7; i++) {
      conns.push({
        a: t[i].id,
        b: t[(i + 1) % t.length].id,
        pipes: [{ id: `pipe-${t[i].id}-${t[(i + 1) % t.length].id}`, dir: 1, status: "active" }],
      });
    }
    return conns;
  };

  const formations = (): ColonyFormation[] =>
    db.swarms().map((s) => ({
      id: s.id,
      name: s.name,
      intent: s.intent.toUpperCase(),
      description: s.intent,
      momentum: s.status === "active" ? 2 : s.status === "paused" ? 1 : 0,
      momentumLabel: s.status === "active" ? "HOT" : s.status === "paused" ? "WARM" : "COLD",
      color: s.status === "active" ? "var(--color-momentum-hot)" : "var(--color-momentum-warm)",
      members: [],
      zone: { x: seeded(s.id + "zx", 20, 80), y: seeded(s.id + "zy", 20, 60) },
    }));

  // ── Command handler ────────────────────────────────────
  const handleCommand = (label: string) => {
    if (label === "SETTINGS") {
      setShowSettings(true);
      setShowSessions(false);
    }
  };

  // ── Overlay content ────────────────────────────────────
  const overlay = () => {
    if (showSettings()) {
      return (
        <div class="mx-auto max-w-md rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="font-bold text-text-primary" style={{ "font-size": "9px" }}>{t("settings.title")}</h2>
            <Show when={connected()}>
              <button onClick={() => setShowSettings(false)} class="text-text-dim hover:text-text-secondary" style={{ "font-size": "6px" }}>✕</button>
            </Show>
          </div>
          <SettingsPage onSaved={handleSettingsSaved} />
        </div>
      );
    }
    if (showSessions()) {
      return (
        <div class="mx-auto max-w-lg rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="font-bold text-text-primary" style={{ "font-size": "9px" }}>{t("sessions.title")}</h2>
            <button onClick={() => setShowSessions(false)} class="text-text-dim hover:text-text-secondary" style={{ "font-size": "6px" }}>✕</button>
          </div>
          <SessionsPage />
        </div>
      );
    }
    return undefined;
  };

  // ── Render ─────────────────────────────────────────────
  return (
    <ColonyShell
      topBar={
        <TopBar
          agents={agents()}
          trees={trees()}
          formations={formations()}
          selection={selection()}
          onSelectAgent={(id) => setSelection({ id, type: "agent" })}
          onSelectFormation={(id) => setSelection({ id, type: "formation" })}
        />
      }
      viewport={
        <Viewport
          trees={trees()}
          agents={agents()}
          connections={connections()}
          formations={formations()}
          events={db.events()}
          selection={selection()}
          onSelectTree={(id) => setSelection({ id, type: "tree" })}
          onSelectAgent={(id) => setSelection({ id, type: "agent" })}
          onSelectFormation={(id) => setSelection({ id, type: "formation" })}
          onClearSelection={() => setSelection({ id: null, type: null })}
          overlay={overlay()}
        />
      }
      bottomPanel={
        <BottomPanel
          trees={trees()}
          agents={agents()}
          connections={connections()}
          formations={formations()}
          selection={selection()}
          onCommand={handleCommand}
        />
      }
    />
  );
};
