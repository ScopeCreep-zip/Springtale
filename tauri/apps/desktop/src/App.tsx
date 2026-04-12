import { createSignal, createEffect, onMount, onCleanup, Show } from "solid-js";
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
  mapNodes,
  mapAgents,
  mapFormations,
} from "@springtale/ui";
import type { ColonySelection, TeamConfig } from "@springtale/ui";
import { COMMANDS } from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { getVaultStatus, unlockVault, createVault } from "./ipc/vault";
import { panicWipe } from "./ipc/panic";
import { resetAutoLock } from "./ipc/autolock";

/**
 * Springtale Desktop — colony ecosystem dashboard.
 *
 * Trees=connectors, springtails=agents, mycelium=pipelines.
 * Full RTS-style command interface with pixel-art visualization.
 */
export const App = () => {
  const db = useDashboard();
  const { t, locale, setLocale } = useI18n();

  // ── Desktop-only: vault + settings state ────────────────
  const [vaultLocked, setVaultLocked] = createSignal(true);
  const [showVault, setShowVault] = createSignal(false);
  const [showDesktopSettings, setShowDesktopSettings] = createSignal(false);
  const [showTeamBuilder, setShowTeamBuilder] = createSignal(false);
  const [passphrase, setPassphrase] = createSignal("");
  const [vaultError, setVaultError] = createSignal("");

  // ── Theme ────────────────────────────────────────────────
  const [theme, setTheme] = createSignal("springtale");
  const applyTheme = (t: string) => {
    document.documentElement.dataset.theme = t;
  };

  // ── Colony state ────────────────────────────────────────
  const [selection, setSelection] = createSignal<ColonySelection>({ id: null, type: null });
  const [connectorPositions, setConnectorPositions] = createSignal<Record<string, { x: number; y: number }>>({});
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
  const [conditionTypes, setConditionTypes] = createSignal<string[]>([]);

  let connectorDragTimer: ReturnType<typeof setTimeout> | undefined;
  const handleConnectorDrag = (id: string, x: number, y: number) => {
    setConnectorPositions((prev) => ({ ...prev, [id]: { x, y } }));
    // Debounced persistence — save to config store after drag settles
    clearTimeout(connectorDragTimer);
    connectorDragTimer = setTimeout(() => {
      db.provider.setConfig("canvas:connector_positions", connectorPositions()).catch(() => {});
    }, 500);
  };

  // ── Persist locale changes to config store ──────────────
  let localeInitialized = false;
  createEffect(() => {
    const loc = locale();
    if (!localeInitialized) { localeInitialized = true; return; }
    db.provider.setConfig("locale", loc).catch(() => {});
  });

  // ── Persist theme changes to config store ───────────────
  let themeInitialized = false;
  createEffect(() => {
    const t = theme();
    applyTheme(t);
    if (!themeInitialized) { themeInitialized = true; return; }
    db.provider.setConfig("theme", t).catch(() => {});
  });

  // ── Data loading (deferred until vault unlocked) ────────
  const loadColonyData = async () => {
    try {
      await db.refresh();
      setAvailableConnectors(await db.provider.listAvailableConnectors());
      setIntents(await db.provider.listIntents());
      const schema = await db.provider.getRuleSchema() as Record<string, Record<string, unknown>>;
      if (schema.conditions) {
        setConditionTypes(Object.keys(schema.conditions));
      }
      setConnections(await db.provider.getConnections() as import("@springtale/ui").ColonyConnection[]);

      try {
        const saved = await db.provider.getConfig("canvas:connector_positions");
        if (saved && typeof saved === "object") {
          setConnectorPositions(saved as Record<string, { x: number; y: number }>);
        }
      } catch { /* No saved positions — seeded defaults */ }

      // Restore persisted locale
      try {
        const savedLocale = await db.provider.getConfig("locale");
        if (savedLocale && typeof savedLocale === "string") {
          setLocale(savedLocale as "en");
        }
      } catch { /* Default locale is fine */ }

      // Restore persisted theme
      try {
        const savedTheme = await db.provider.getConfig("theme");
        if (savedTheme && typeof savedTheme === "string") {
          setTheme(savedTheme);
        }
      } catch { /* Default theme is fine */ }
    } catch (e) {
      console.warn("loadColonyData:", e);
    }
  };

  // ── Auto-lock (Rust backend) ───────────────────────────
  const resetTimer = () => { resetAutoLock().catch(() => {}); };

  onMount(async () => {
    document.addEventListener("mousemove", resetTimer);
    document.addEventListener("keydown", resetTimer);
    document.addEventListener("click", resetTimer);
    resetTimer();

    // Keyboard shortcuts
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
      const key = e.key.toLowerCase();

      // Quick-exit: Ctrl+Shift+Q — instant hide + auto-lock
      // For IPV survivors who need to hide the app immediately
      if (key === "q" && e.ctrlKey && e.shiftKey) {
        e.preventDefault();
        invoke("lock_vault").catch(() => {});
        invoke("plugin:window|hide").catch(() => {});
        return;
      }

      // 1-9: select agent by index
      if (key >= "1" && key <= "9") {
        const idx = parseInt(key) - 1;
        const a = agents();
        const agent = a[idx];
        if (agent) setSelection({ id: agent.id, type: "agent" });
        return;
      }

      // Escape: clear selection and close overlays
      if (key === "escape") {
        setSelection({ id: null, type: null });
        setConfirmAction(null);
        setShowDesktopSettings(false);
        setShowTeamBuilder(false);
        setAiConfigAgent(null);
        setConnectorConfigData(null);
        return;
      }

      // Skip command shortcuts when any modal is open
      if (showVault() || showDesktopSettings() || showTeamBuilder() || confirmAction() || aiConfigAgent() || connectorConfigData()) return;

      // Command grid shortcuts: match key to current selection context
      const context = selection().type ?? "none";
      const commandList = COMMANDS[context] ?? COMMANDS.none;
      const cmd = commandList.find((c) => c?.key.toLowerCase() === key);
      if (cmd) {
        e.preventDefault();
        handleCommand(cmd.action);
      }
    };
    document.addEventListener("keydown", handleKeyDown);

    await listen("vault-locked", () => {
      setVaultLocked(true);
      setShowVault(true);
    });

    await listen("vault-unlocked", async () => {
      setVaultLocked(false);
      setShowVault(false);
      await loadColonyData();
    });

    try {
      const v = await getVaultStatus();
      setVaultLocked(!v.unlocked);
      if (!v.unlocked) setShowVault(true);
    } catch (e) {
      const msg = String(e);
      if (msg.includes("vault file not found")) {
        setShowVault(true);
        setVaultLocked(true);
      }
    }

    // Only load colony data if the vault is already unlocked.
    // On fresh start the vault is locked — data loading happens
    // when the "vault-unlocked" event fires (see listener above).
    if (!vaultLocked()) {
      await loadColonyData();
    }
  });

  onCleanup(() => {
    document.removeEventListener("mousemove", resetTimer);
    document.removeEventListener("keydown", resetTimer);
    document.removeEventListener("click", resetTimer);
  });

  // ── Vault handlers ─────────────────────────────────────
  const handleUnlock = async () => {
    try {
      const result = await unlockVault(passphrase());
      setVaultLocked(!result.unlocked);
      if (result.unlocked) setShowVault(false);
      setPassphrase("");
      setVaultError("");
      await db.refresh();
    } catch (e) {
      setVaultError(String(e));
    }
  };

  const handleCreateVault = async () => {
    try {
      const result = await createVault(passphrase());
      setVaultLocked(!result.unlocked);
      if (result.unlocked) setShowVault(false);
      setPassphrase("");
      setVaultError("");
      await db.refresh();
    } catch (e) {
      setVaultError(String(e));
    }
  };

  // ── Data → Colony visual model (real data, no fakes) ───
  const nodes = () => mapNodes(db.connectors());
  const agents = () => mapAgents(db.rules(), db.agentStates());
  const [connections, setConnections] = createSignal<import("@springtale/ui").ColonyConnection[]>([]);
  const formations = () => mapFormations(db.swarms());

  // ── Command dispatch — context:action pattern ───────────
  // Each command has a unique action ID (e.g. "connector:enable", "agent:pause")
  // so there's no ambiguity between contexts. Every case maps to a real
  // backend operation or opens a real UI panel.
  const handleCommand = async (action: string) => {
    const sel = selection();
    try {
      switch (action) {
        // ── Global ──
        case "global:new_rule":
          setShowTeamBuilder(true);
          break;
        case "global:refresh":
          await db.refresh();
          setConnections(await db.provider.getConnections() as import("@springtale/ui").ColonyConnection[]);
          break;
        case "global:connectors":
          setSelection({ id: null, type: null });
          setDetailView({ mode: "connectors" });
          setAvailableConnectors(await db.provider.listAvailableConnectors());
          await db.refresh();
          break;
        case "global:events":
          setSelection({ id: null, type: null });
          setDetailView({ mode: "events" });
          break;
        case "global:bots":
          setSelection({ id: null, type: null });
          setDetailView({ mode: "bots" });
          break;
        case "global:settings":
          setShowDesktopSettings(true);
          break;

        // ── Tree (connector selected) ──
        case "connector:enable":
          if (sel.id) { await db.provider.enableConnector(sel.id); await db.refresh(); }
          break;
        case "connector:disable":
          if (sel.id) { await db.provider.disableConnector(sel.id); await db.refresh(); }
          break;
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

        // ── Agent (bot selected) ──
        case "agent:ai_config":
          if (sel.id) {
            const agent = agents().find((a) => a.id === sel.id);
            setAiConfigAgent({ id: sel.id, name: agent?.name ?? sel.id });
          }
          break;
        case "agent:pause":
          if (sel.id) { await db.handleToggle(sel.id, true); await db.refresh(); }
          break;
        case "agent:recall":
          // Recall = disable + clear selection (agent goes idle, returns to tree)
          if (sel.id) { await db.handleToggle(sel.id, true); setSelection({ id: null, type: null }); await db.refresh(); }
          break;
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
          if (sel.id) {
            setDetailView({ mode: "formations", addAgentId: sel.id });
          }
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

        // ── Formation (swarm selected) ──
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
        case "formation:rally":
          if (sel.id) { await db.handleResumeFormation(sel.id); await db.refresh(); }
          break;
        case "formation:ai_config":
          if (sel.id) {
            const fm = db.swarms().find((s) => s.id === sel.id);
            setAiConfigAgent({ id: `formation:${sel.id}`, name: fm?.name ?? sel.id });
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
          // Show entity detail with fuel info
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

  // ── Unified overlay — priority order ────────────────────
  const shellOverlay = () => {
    // 1. Vault (security — blocks everything else)
    if (showVault()) {
      return (
        <div class="colony-modal mx-auto max-w-lg space-y-5 overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <h2 class="colony-text-md font-bold text-text-primary">{t("vault.title")}</h2>
          <p class="colony-text-xs text-text-dim">{vaultLocked() ? t("vault.createDesc") : ""}</p>
          {vaultError() && <div class="colony-text-2xs border border-status-error bg-status-error/10 p-2 text-status-error">{vaultError()}</div>}
          <div>
            <label for="vault-pass" class="colony-text-2xs text-text-secondary">{t("vault.passphrase")}</label>
            <input
              id="vault-pass" type="password" value={passphrase()}
              onInput={(e) => setPassphrase(e.currentTarget.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleUnlock(); }}
              class="colony-text-xs mt-2 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
            />
          </div>
          <div class="flex gap-3">
            <button onClick={handleUnlock} class="colony-text-2xs border-2 border-status-ok bg-soil-light px-4 py-2 text-status-ok hover:bg-soil-deep">{t("vault.unlock")}</button>
            <button onClick={handleCreateVault} class="colony-text-2xs border-2 border-bark bg-soil-light px-4 py-2 text-text-secondary hover:bg-soil-deep">{t("vault.create")}</button>
            <Show when={!vaultLocked()}>
              <button onClick={() => setShowVault(false)} class="colony-close-btn">✕</button>
            </Show>
          </div>
        </div>
      );
    }

    // 2. Confirm dialog (destructive actions)
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
            >{ca.label}</button>
            <button class="colony-command-btn colony-text-2xs px-4 py-2" onClick={() => setConfirmAction(null)}>Cancel</button>
          </div>
        </div>
      );
    }

    // 3. Per-bot AI config (agent:ai_config command)
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

    // 4. Connector config — full management panel
    if (connectorConfigData()) {
      const ccd = connectorConfigData()!;
      return (
        <ConnectorConfigPanel
          connectorId={ccd.id}
          schemas={db.schemas()}
          conditionTypes={conditionTypes()}
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

    // 5. App settings
    if (showDesktopSettings()) {
      return (
        <AppSettingsPanel
          isDesktop={true}
          onVault={() => { setShowDesktopSettings(false); setShowVault(true); }}
          onPanicWipe={async () => {
            try {
              const { ask } = await import("@tauri-apps/plugin-dialog");
              const ok = await ask("This will irreversibly wipe all data. Are you sure?", { kind: "warning" });
              if (ok) await panicWipe();
            } catch (e) {
              db.setError(String(e));
            }
          }}
          onExportData={async () => { await db.provider.exportData(); }}
          onCompactMemory={async () => { await db.provider.compactMemory(1000); }}
          onClose={() => setShowDesktopSettings(false)}
          theme={theme()}
          onThemeChange={(t) => setTheme(t)}
        />
      );
    }

    // 6. TeamBuilder OOBE — full-panel overlay like settings
    if (showTeamBuilder()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
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

    return undefined;
  };

  // ── Render ─────────────────────────────────────────────
  return (
    <ColonyShell
      topBar={
        <TopBar
          agents={agents()}
          nodes={nodes()}
          formations={formations()}
          events={db.events()}
          selection={selection()}
          onSelectAgent={(id) => { setSelection({ id, type: "agent" }); setDetailView({ mode: "entity" }); }}
          onSelectFormation={(id) => { setSelection({ id, type: "formation" }); setDetailView({ mode: "entity" }); }}
        />
      }
      viewport={
        <Viewport
          nodes={nodes()}
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
          nodes={nodes()}
          agents={agents()}
          connections={connections()}
          formations={formations()}
          connectorPositions={connectorPositions()}
          outputs={connectorOutputs() as any}
          availableConnectors={availableConnectors()}
          events={db.events()}
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
