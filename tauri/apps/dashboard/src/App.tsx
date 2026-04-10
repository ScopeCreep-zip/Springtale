import { createSignal, onMount, onCleanup, Show } from "solid-js";
import {
  ColonyShell,
  TopBar,
  Viewport,
  BottomPanel,
  TeamBuilder,
  AiConfigPanel,
  AppSettingsPanel,
  ConnectorConfigPanel,
  useDashboard,
  useI18n,
  mapTrees,
  mapAgents,
  mapConnections,
  mapFormations,
} from "@springtale/ui";
import type { ColonySelection, TeamConfig } from "@springtale/ui";
import { configure } from "./api/client";
import { SettingsPage } from "./pages/Settings";
import { SessionsPage } from "./pages/Sessions";

/**
 * Springtale Web Dashboard — colony ecosystem view.
 *
 * Same visual as the desktop app. Settings/sessions render
 * as full-screen overlays via ColonyShell.
 */
export const App = () => {
  const db = useDashboard();
  const { t } = useI18n();

  // ── Colony state ────────────────────────────────────────
  const [selection, setSelection] = createSignal<ColonySelection>({ id: null, type: null });
  const [connectorPositions, setConnectorPositions] = createSignal<Record<string, { x: number; y: number }>>({});
  const [showSettings, setShowSettings] = createSignal(false);
  const [showSessions, setShowSessions] = createSignal(false);
  const [showTeamBuilder, setShowTeamBuilder] = createSignal(false);
  const [connected, setConnected] = createSignal(false);
  const [confirmAction, setConfirmAction] = createSignal<{
    title: string; message: string; label: string; action: () => Promise<void>;
  } | null>(null);
  const [aiConfigAgent, setAiConfigAgent] = createSignal<{ id: string; name: string } | null>(null);
  const [detailView, setDetailView] = createSignal<import("@springtale/ui").DetailView>({ mode: "colony" });
  const [pendingAddToFormation, setPendingAddToFormation] = createSignal<string | null>(null);
  const [pendingReassignAgent, setPendingReassignAgent] = createSignal<string | null>(null);
  const [connectorConfigData, setConnectorConfigData] = createSignal<{ id: string; config: unknown; configSchema?: import("@springtale/types").ConfigSchema } | null>(null);
  const [notification, setNotification] = createSignal<{ message: string; type: "ok" | "warn" } | null>(null);
  const [connectorOutputs, setConnectorOutputs] = createSignal<unknown[]>([]);
  const [availableConnectors, setAvailableConnectors] = createSignal<import("@springtale/types").AvailableConnector[]>([]);
  const [intents, setIntents] = createSignal<Array<{ value: string; label: string }>>([]);

  let connectorDragTimer: ReturnType<typeof setTimeout> | undefined;
  const handleConnectorDrag = (id: string, x: number, y: number) => {
    setConnectorPositions((prev) => ({ ...prev, [id]: { x, y } }));
    clearTimeout(connectorDragTimer);
    connectorDragTimer = setTimeout(() => {
      db.provider.setConfig("canvas:connector_positions", connectorPositions()).catch(() => {});
    }, 500);
  };

  const handleSettingsSaved = async () => {
    setConnected(true);
    db.resubscribe();
    await db.refresh();
    setAvailableConnectors(await db.provider.listAvailableConnectors());
    setIntents(await db.provider.listIntents());

    // Load saved tree positions
    try {
      const saved = await db.provider.getConfig("canvas:connector_positions");
      if (saved && typeof saved === "object") {
        setConnectorPositions(saved as Record<string, { x: number; y: number }>);
      }
    } catch { /* seeded defaults will be used */ }
  };

  // ── Keyboard shortcuts ─────────────────────────────────
  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
      const key = e.key.toLowerCase();
      if (key >= "1" && key <= "9") {
        const idx = parseInt(key) - 1;
        const a = agents();
        const agent = a[idx];
        if (agent) setSelection({ id: agent.id, type: "agent" });
      }
      if (key === "escape") {
        setSelection({ id: null, type: null });
        setConfirmAction(null);
        setShowSettings(false);
        setShowSessions(false);
        setShowTeamBuilder(false);
        setAiConfigAgent(null);
        setConnectorConfigData(null);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    onCleanup(() => document.removeEventListener("keydown", handleKeyDown));
  });

  // ── Data → Colony visual model (real data, no fakes) ───
  const trees = () => mapTrees(db.connectors());
  const agents = () => mapAgents(db.rules(), db.agentStates());
  const connections = () => mapConnections(trees(), db.schemas(), db.rules());
  const formations = () => mapFormations(db.swarms());

  // ── Command dispatch — context:action pattern ───────────
  const handleCommand = async (action: string) => {
    const sel = selection();
    try {
      switch (action) {
        // ── Global ──
        case "global:new_rule": setShowTeamBuilder(true); break;
        case "global:refresh": await db.refresh(); break;
        case "global:connectors":
          setSelection({ id: null, type: null });
          setDetailView({ mode: "connectors" });
          setAvailableConnectors(await db.provider.listAvailableConnectors());
          await db.refresh();
          break;
        case "global:events": setSelection({ id: null, type: null }); setDetailView({ mode: "events" }); break;
        case "global:bots": setSelection({ id: null, type: null }); setDetailView({ mode: "bots" }); break;
        case "global:settings": setShowSettings(true); setShowSessions(false); break;

        // ── Tree ──
        case "connector:enable": if (sel.id) { await db.provider.enableConnector(sel.id); await db.refresh(); } break;
        case "connector:disable": if (sel.id) { await db.provider.disableConnector(sel.id); await db.refresh(); } break;
        case "connector:config":
          if (sel.id) {
            const config = await db.provider.getConnectorConfig(sel.id);
            const avail = availableConnectors().find((a) => a.name === sel.id);
            setConnectorConfigData({ id: sel.id, config, configSchema: avail?.config_schema });
          }
          break;
        case "connector:remove":
          if (sel.id) {
            const deps = await db.provider.listRulesForConnector(sel.id);
            setConfirmAction({
              title: "Remove Connector",
              message: `Remove ${sel.id} and ${deps.length} dependent rule(s)?`,
              label: "Remove",
              action: async () => {
                await db.provider.removeConnectorCascade(sel.id!);
                setSelection({ id: null, type: null });
                await db.refresh();
              },
            });
          }
          break;
        case "connector:events":
          if (sel.id) { setDetailView({ mode: "events", filterConnector: sel.id }); }
          break;
        case "connector:test":
          if (sel.id) {
            try {
              const result = await db.provider.testConnector(sel.id);
              setNotification({
                message: result.matched ? `Test passed: "${result.rule_name}"` : `No match: "${result.rule_name}"`,
                type: result.matched ? "ok" : "warn",
              });
            } catch {
              setNotification({ message: `No rules for ${sel.id}`, type: "warn" });
            }
            setTimeout(() => setNotification(null), 4000);
          }
          break;
        case "connector:outputs":
          if (sel.id) {
            const outputs = await db.provider.listConnectorOutputs(sel.id);
            setConnectorOutputs(outputs);
            setDetailView({ mode: "outputs", connectorId: sel.id });
          }
          break;

        // ── Agent ──
        case "agent:ai_config":
          if (sel.id) {
            const agentForAi = agents().find((a) => a.id === sel.id);
            setAiConfigAgent({ id: sel.id, name: agentForAi?.name ?? sel.id });
          }
          break;
        case "agent:pause": if (sel.id) { await db.handleToggle(sel.id, true); await db.refresh(); } break;
        case "agent:recall": if (sel.id) { await db.handleToggle(sel.id, true); setSelection({ id: null, type: null }); await db.refresh(); } break;
        case "agent:detach":
          if (sel.id) {
            setConfirmAction({
              title: "Detach Agent",
              message: "This will delete the rule. The agent will be removed from the colony.",
              label: "Detach",
              action: async () => { await db.handleDelete(sel.id!); setSelection({ id: null, type: null }); await db.refresh(); },
            });
          }
          break;
        case "agent:inspect":
          setDetailView({ mode: "entity" });
          break;
        case "agent:group":
          if (sel.id) { setDetailView({ mode: "formations", addAgentId: sel.id }); }
          break;
        case "agent:reassign":
          if (sel.id) {
            setPendingReassignAgent(sel.id);
            setDetailView({ mode: "connectors" });
          }
          break;
        case "agent:autonomy_up":
          if (sel.id) { await db.provider.stepAutonomy(sel.id, "up"); await db.refresh(); }
          break;
        case "agent:autonomy_down":
          if (sel.id) { await db.provider.stepAutonomy(sel.id, "down"); await db.refresh(); }
          break;

        // ── Formation ──
        case "formation:dissolve":
          if (sel.id) {
            setConfirmAction({
              title: "Dissolve Formation",
              message: "All agents will be released from this formation.",
              label: "Dissolve",
              action: async () => { await db.handleDissolveFormation(sel.id!); setSelection({ id: null, type: null }); await db.refresh(); },
            });
          }
          break;
        case "formation:rally": if (sel.id) { await db.handleResumeFormation(sel.id); await db.refresh(); } break;
        case "formation:ai_config":
          if (sel.id) {
            const fmAi = db.swarms().find((s) => s.id === sel.id);
            setAiConfigAgent({ id: `formation:${sel.id}`, name: fmAi?.name ?? sel.id });
          }
          break;
        case "formation:autonomy":
          if (sel.id) { await db.provider.cycleFormationAutonomy(sel.id); await db.refresh(); }
          break;
        case "formation:intent":
          if (sel.id) { await db.provider.cycleFormationIntent(sel.id); await db.refresh(); }
          break;
        case "formation:add":
          if (sel.id) {
            setPendingAddToFormation(sel.id);
            setDetailView({ mode: "connectors" });
          }
          break;
        case "formation:fuel":
          setDetailView({ mode: "entity" });
          break;
        case "formation:guard":
          if (sel.id) { await db.provider.toggleFormationGuard(sel.id); await db.refresh(); }
          break;
      }
    } catch (e) {
      db.setError(String(e));
    }
  };

  // ── Unified overlay — settings → confirm → hatch → sessions ──
  const shellOverlay = () => {
    if (showSettings()) {
      return (
        <div class="mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6" style={{ "max-height": "80vh" }}>
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">{t("settings.title")}</h2>
            <Show when={connected()}>
              <button onClick={() => setShowSettings(false)} class="colony-close-btn">✕</button>
            </Show>
          </div>
          <SettingsPage onSaved={handleSettingsSaved} />
        </div>
      );
    }

    if (confirmAction()) {
      const ca = confirmAction()!;
      return (
        <div class="mx-auto max-w-lg rounded border-2 border-bark bg-soil-mid p-6 text-center">
          <p class="colony-text-md font-bold text-text-primary">{ca.title}</p>
          <p class="colony-text-xs mt-2 text-text-secondary">{ca.message}</p>
          <div class="mt-4 flex justify-center gap-3">
            <button
              class="colony-command-btn colony-text-2xs px-4 py-2"
              style={{ "border-color": "var(--color-status-error)" }}
              onClick={async () => {
                try { await ca.action(); setConfirmAction(null); }
                catch (e) { db.setError(String(e)); setConfirmAction(null); }
              }}
            >
              {ca.label}
            </button>
            <button class="colony-command-btn colony-text-2xs px-4 py-2" onClick={() => setConfirmAction(null)}>
              Cancel
            </button>
          </div>
        </div>
      );
    }

    // Per-bot AI config
    if (aiConfigAgent()) {
      const aca = aiConfigAgent()!;
      return (
        <AiConfigPanel
          agentId={aca.id}
          agentName={aca.name}
          onSave={async (agentId, config) => {
            await db.provider.configureAiAdapter(`ai:${agentId}`, config);
            await db.refresh();
          }}
          onClose={() => setAiConfigAgent(null)}
        />
      );
    }

    // Connector config — full management panel
    if (connectorConfigData()) {
      const ccd = connectorConfigData()!;
      return (
        <ConnectorConfigPanel
          connectorId={ccd.id}
          schemas={db.schemas()}
          rules={db.rules().filter((r) => (r.connector ?? r.triggerType) === ccd.id)}
          currentConfig={ccd.config}
          configSchema={ccd.configSchema}
          onSave={async (id, config) => {
            await db.provider.upsertConnectorConfig(id, config);
            setAvailableConnectors(await db.provider.listAvailableConnectors());
            await db.refresh();
          }}
          onToggleRule={async (ruleId, enabled) => {
            await db.handleToggle(ruleId, enabled);
            await db.refresh();
          }}
          onDeleteRule={async (ruleId) => {
            await db.handleDelete(ruleId);
            await db.refresh();
          }}
          onCreateRule={async (rule) => {
            await db.provider.createConnectorRule(rule);
            await db.refresh();
          }}
          onTest={async (id) => {
            const result = await db.provider.testConnector(id);
            if (!result.matched) throw new Error("Rule did not match");
          }}
          onClose={() => setConnectorConfigData(null)}
        />
      );
    }

    // TeamBuilder OOBE — full-panel overlay like settings
    if (showTeamBuilder()) {
      return (
        <div class="mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6" style={{ "max-height": "80vh" }}>
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">Build Your Team</h2>
            <button onClick={() => setShowTeamBuilder(false)} class="colony-close-btn">✕</button>
          </div>
          <TeamBuilder
            availableConnectors={availableConnectors()}
            connectors={db.schemas()}
            intents={intents()}
            onSetupConnector={(name) => {
              const avail = availableConnectors().find((a) => a.name === name);
              setConnectorConfigData({ id: name, config: {}, configSchema: avail?.config_schema });
            }}
            onParseRule={async (intent) => db.provider.parseRuleFromIntent(intent)}
            onSaveAiConfig={async (config) => {
              await db.provider.configureAiAdapter("ai:global", config);
            }}
            onDeploy={async (team: TeamConfig) => {
              await db.provider.deployTeam({
                name: team.name,
                intent: team.intent,
                guard_mode: team.guardMode,
                agents: team.agents.map((a) => ({
                  connector_name: a.connectorName,
                  trigger_name: a.triggerName,
                  action_connector: a.actionConnector,
                  action_name: a.actionName,
                })),
              });
              setShowTeamBuilder(false);
              await db.refresh();
              setAvailableConnectors(await db.provider.listAvailableConnectors());
            }}
            onCancel={() => setShowTeamBuilder(false)}
          />
        </div>
      );
    }

    if (showSessions()) {
      return (
        <div class="mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6" style={{ "max-height": "80vh" }}>
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">{t("sessions.title")}</h2>
            <button onClick={() => setShowSessions(false)} class="colony-close-btn">✕</button>
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
          events={db.events()}
          selection={selection()}
          onSelectAgent={(id) => { setSelection({ id, type: "agent" }); setDetailView({ mode: "entity" }); }}
          onSelectFormation={(id) => { setSelection({ id, type: "formation" }); setDetailView({ mode: "entity" }); }}
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
          onSelectConnector={(id) => { setSelection({ id, type: "connector" }); setDetailView({ mode: "entity" }); }}
          onSelectAgent={(id) => { setSelection({ id, type: "agent" }); setDetailView({ mode: "entity" }); }}
          onSelectFormation={(id) => { setSelection({ id, type: "formation" }); setDetailView({ mode: "entity" }); }}
          onClearSelection={() => setSelection({ id: null, type: null })}
          connectorPositions={connectorPositions()}
          onConnectorDrag={handleConnectorDrag}
          onHatch={() => setShowTeamBuilder(true)}
          availableConnectors={availableConnectors()}
          connectorSchemas={db.schemas()}
          onSetupConnector={(name) => {
            const avail = availableConnectors().find((a) => a.name === name);
            setConnectorConfigData({ id: name, config: {}, configSchema: avail?.config_schema });
          }}
          onParseRule={async (intent) => db.provider.parseRuleFromIntent(intent)}
        />
      }
      bottomPanel={
        <BottomPanel
          trees={trees()}
          agents={agents()}
          connections={connections()}
          formations={formations()}
          connectorPositions={connectorPositions()}
          events={db.events()}
          outputs={connectorOutputs() as any}
          availableConnectors={availableConnectors()}
          selection={selection()}
          detailView={detailView()}
          onCommand={handleCommand}
          onSelectAgent={(id) => { setSelection({ id, type: "agent" }); setDetailView({ mode: "entity" }); }}
          onSelectConnector={async (id) => {
            const reassignId = pendingReassignAgent();
            const formationId = pendingAddToFormation();
            if (reassignId) {
              await db.provider.reassignRuleConnector(reassignId, id);
              setPendingReassignAgent(null);
              setDetailView({ mode: "entity" });
              await db.refresh();
            } else if (formationId) {
              await db.provider.addFormationMember(formationId, id);
              setPendingAddToFormation(null);
              setDetailView({ mode: "entity" });
              await db.refresh();
            } else {
              setSelection({ id, type: "connector" });
              setDetailView({ mode: "entity" });
            }
          }}
          onAddToFormation={async (fId, cName) => {
            await db.provider.addFormationMember(fId, cName);
            setDetailView({ mode: "entity" });
            await db.refresh();
          }}
          onSetupConnector={(name) => {
            const avail = availableConnectors().find((a) => a.name === name);
            setConnectorConfigData({ id: name, config: {}, configSchema: avail?.config_schema });
          }}
          onCreateBot={() => setShowTeamBuilder(true)}
        />
      }
      overlay={shellOverlay()}
      notification={notification() ?? undefined}
    />
  );
};
