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
  MemberPickerOverlay,
  ModeSelectOverlay,
  ProofOfLifePanel,
  RecipeAuthorPanel,
  RecipeDeployPanel,
  RecipeLibraryOverlay,
  RuleBuilderOverlay,
  useDashboard,
  useI18n,
  mapNodes,
  mapAgents,
  mapFormations,
} from "@springtale/ui";
import type { ColonySelection, CreateMode, Recipe, RecipeApplyReport, RecipeLibraryVariant, TeamBuilderSeed, TeamConfig } from "@springtale/ui";
import { COMMANDS } from "@springtale/ui";
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
  const { t, locale, setLocale } = useI18n();

  // ── Theme ────────────────────────────────────────────────
  const [theme, setTheme] = createSignal("springtale");
  const applyTheme = (t: string) => {
    document.documentElement.dataset.theme = t;
  };

  // ── Colony state ────────────────────────────────────────
  const [selection, setSelection] = createSignal<ColonySelection>({ id: null, type: null });
  const [connectorPositions, setConnectorPositions] = createSignal<Record<string, { x: number; y: number }>>({});
  const [showSettings, setShowSettings] = createSignal(false);
  // F5 — member-picker overlay state.
  const [memberPickerFor, setMemberPickerFor] = createSignal<string | null>(null);
  const [showSessions, setShowSessions] = createSignal(false);
  const [showTeamBuilder, setShowTeamBuilder] = createSignal(false);
  const [showRuleBuilder, setShowRuleBuilder] = createSignal(false);
  // W1.A — mode-select hub before any compose flow.
  const [showModeSelect, setShowModeSelect] = createSignal(false);
  // W1.B — recipe library opens after mode-select. null = closed.
  const [recipeLibraryVariant, setRecipeLibraryVariant] = createSignal<RecipeLibraryVariant | null>(null);
  const [connected, setConnected] = createSignal(false);
  const [confirmAction, setConfirmAction] = createSignal<{
    title: string; message: string; label: string; action: () => Promise<void>;
  } | null>(null);
  const [aiConfigAgent, setAiConfigAgent] = createSignal<{ id: string; name: string; scope: "agent" | "formation" } | null>(null);
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

  const handleSettingsSaved = async () => {
    setConnected(true);
    db.resubscribe();
    await db.refresh();
    setAvailableConnectors(await db.provider.listAvailableConnectors());
    setIntents(await db.provider.listIntents());
    const schema = await db.provider.getRuleSchema() as Record<string, Record<string, unknown>>;
    if (schema.conditions) {
      setConditionTypes(Object.keys(schema.conditions));
    }
    setConnections(await db.provider.getConnections() as import("@springtale/ui").ColonyConnection[]);

    // Load saved tree positions
    try {
      const saved = await db.provider.getConfig("canvas:connector_positions");
      if (saved && typeof saved === "object") {
        setConnectorPositions(saved as Record<string, { x: number; y: number }>);
      }
    } catch { /* seeded defaults will be used */ }

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
  };

  // ── Keyboard shortcuts ─────────────────────────────────
  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
      const key = e.key.toLowerCase();

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
        setShowSettings(false);
        setShowSessions(false);
        setShowTeamBuilder(false);
        setShowModeSelect(false);
        setAiConfigAgent(null);
        setConnectorConfigData(null);
        return;
      }

      // W2.E — `O` toggles the canvas (OUTPUT) view.
      if (key === "o" && !e.ctrlKey && !e.metaKey && !e.shiftKey
          && !showSettings() && !showSessions() && !showTeamBuilder()
          && !showRuleBuilder() && !showModeSelect() && recipeLibraryVariant() === null
          && recipeDeploy() === null && proofOfLife() === null
          && !confirmAction() && !aiConfigAgent() && !connectorConfigData()) {
        e.preventDefault();
        setDetailView({ mode: "canvas" });
        return;
      }

      // W1.A — `N` opens the mode-select hub.
      if (key === "n" && !e.ctrlKey && !e.metaKey && !e.shiftKey
          && !showSettings() && !showSessions() && !showTeamBuilder()
          && !showRuleBuilder() && !showModeSelect() && recipeLibraryVariant() === null
          && recipeDeploy() === null
          && !confirmAction() && !aiConfigAgent() && !connectorConfigData()) {
        e.preventDefault();
        setShowModeSelect(true);
        return;
      }

      // Skip command shortcuts when any modal is open
      if (showSettings() || showSessions() || showTeamBuilder() || showRuleBuilder() || showModeSelect() || recipeLibraryVariant() !== null || recipeDeploy() !== null || proofOfLife() !== null || recipeAuthorDraft() !== null || confirmAction() || aiConfigAgent() || connectorConfigData()) return;

      // Command grid shortcuts: match key to current selection context.
      // F1: formation hotkeys come exclusively from the backend
      // (`provider.formationAvailableCommands(id)`); there is no
      // hardcoded fallback. If the resource hasn't resolved yet, short-
      // circuit so global hotkeys don't fire while a formation is
      // selected (a stray "R" press shouldn't trigger REFRESH when the
      // user expected RALLY).
      const context = selection().type ?? "none";
      if (context === "formation") {
        const fmCmds = db.formationCommands();
        if (!fmCmds) return;
        const fc = fmCmds.find((c) => c.enabled && c.hotkey.toLowerCase() === key);
        if (fc) {
          e.preventDefault();
          handleCommand(fc.id);
        }
        return;
      }
      const commandList = COMMANDS[context] ?? COMMANDS.none ?? [];
      const cmd = commandList.find((c) => c?.key.toLowerCase() === key);
      if (cmd) {
        e.preventDefault();
        handleCommand(cmd.action);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    onCleanup(() => document.removeEventListener("keydown", handleKeyDown));
  });

  // ── Data → Colony visual model (real data, no fakes) ───
  const nodes = () => mapNodes(db.connectors());
  const agents = () => mapAgents(db.rules(), db.agentStates());
  const [connections, setConnections] = createSignal<import("@springtale/ui").ColonyConnection[]>([]);
  const formations = () => mapFormations(db.swarms(), db.cooperationEvents());

  // ── Command dispatch — context:action pattern ───────────
  const handleCommand = async (action: string) => {
    const sel = selection();
    try {
      switch (action) {
        // ── Global ──
        case "global:new_rule":
          // G5e — open the visual rule builder. TeamBuilder remains
          // available via the formation-creation entry; this command
          // targets the rule-composition surface specifically.
          setShowRuleBuilder(true);
          break;
        case "global:make_bot":
          // Canvas is live so a refresh command is redundant — this slot
          // now routes the user back to the bot/team selection hub.
          setSelection({ id: null, type: null });
          setShowModeSelect(true);
          break;
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
            setAiConfigAgent({ id: sel.id, name: agentForAi?.name ?? sel.id, scope: "agent" });
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
        case "formation:deploy":
          if (sel.id) { await db.handleDeployFormation(sel.id); await db.refresh(); }
          break;
        case "formation:pause":
          if (sel.id) { await db.handlePauseFormation(sel.id); await db.refresh(); }
          break;
        case "formation:resume":
          if (sel.id) { await db.handleResumeFormation(sel.id); await db.refresh(); }
          break;
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
        case "formation:remove_member":
          // F5 — open the member-picker overlay scoped to this formation.
          if (sel.id) { setMemberPickerFor(sel.id); }
          break;
        case "formation:rally": if (sel.id) { await db.handleRallyFormation(sel.id); await db.refresh(); } break;
        case "formation:ai_config":
        case "formation:ai_adapter":
          // G7 — per-formation AI override panel. Saves config under
          // `ai:formation:{id}`; the next Fever-tier orchestrate call
          // resolves it via the agent→formation→global precedence chain.
          if (sel.id) {
            const fmAi = db.swarms().find((s) => s.id === sel.id);
            setAiConfigAgent({ id: sel.id, name: fmAi?.name ?? sel.id, scope: "formation" });
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

  // W1.A — mode-select dispatch. Each card opens the recipe library
  // (W1.B) scoped to the chosen variant. The library has a "Build
  // from scratch" escape hatch that falls through to TeamBuilder.
  const handleModeSelect = (mode: CreateMode) => {
    setShowModeSelect(false);
    switch (mode) {
      case "bot":
        setRecipeLibraryVariant("bot");
        break;
      case "team":
        setRecipeLibraryVariant("team");
        break;
      case "addToTeam":
        setRecipeLibraryVariant("all");
        break;
    }
  };

  // W1.C — recipe deploy panel. When set, replaces the library
  // overlay until the user deploys or cancels.
  const [recipeDeploy, setRecipeDeploy] = createSignal<Recipe | null>(null);
  // W1.E — post-deploy proof-of-life panel.
  const [proofOfLife, setProofOfLife] = createSignal<RecipeApplyReport | null>(null);
  // W2.A — seed for TeamBuilder when user picks CUSTOMIZE on a recipe.
  const [teamBuilderSeed, setTeamBuilderSeed] = createSignal<TeamBuilderSeed | null>(null);
  // W2.B — recipe authoring panel draft.
  const [recipeAuthorDraft, setRecipeAuthorDraft] = createSignal<Recipe | null>(null);
  const handleUseRecipe = (recipe: Recipe) => {
    setRecipeLibraryVariant(null);
    setRecipeDeploy(recipe);
  };

  // ── Unified overlay — settings → confirm → hatch → sessions ──
  const shellOverlay = () => {
    // F5 — member picker takes precedence so destructive flow gets focus.
    const pickerFor = memberPickerFor();
    if (pickerFor) {
      return (
        <MemberPickerOverlay
          formationId={pickerFor}
          onRemoved={async () => { await db.refresh(); }}
          onCancel={() => setMemberPickerFor(null)}
        />
      );
    }
    // W1.A — Mode select hub before any compose flow.
    if (showModeSelect()) {
      return (
        <ModeSelectOverlay
          hasExistingTeams={(db.swarms()?.length ?? 0) > 0}
          onSelectMode={handleModeSelect}
          onCancel={() => setShowModeSelect(false)}
        />
      );
    }
    // W1.B — Recipe library opens from any mode-select card.
    if (recipeLibraryVariant() !== null) {
      return (
        <RecipeLibraryOverlay
          variant={recipeLibraryVariant() ?? "all"}
          favorites={new Set<string>()}
          onSelectRecipe={handleUseRecipe}
          onToggleFavorite={() => {}}
          onBuildFromScratch={() => {
            setRecipeLibraryVariant(null);
            setTeamBuilderSeed(null);
            setShowTeamBuilder(true);
          }}
          onCancel={() => setRecipeLibraryVariant(null)}
        />
      );
    }
    // W1.C — recipe deploy panel.
    const deployRecipe = recipeDeploy();
    if (deployRecipe) {
      return (
        <RecipeDeployPanel
          recipe={deployRecipe}
          onDeployed={async (report) => {
            setRecipeDeploy(null);
            await db.refresh();
            setProofOfLife(report);
          }}
          onCancel={() => setRecipeDeploy(null)}
        />
      );
    }
    // W1.E — proof-of-life panel.
    const polReport = proofOfLife();
    if (polReport) {
      return (
        <ProofOfLifePanel
          report={polReport}
          onDismiss={() => setProofOfLife(null)}
        />
      );
    }
    // W2.B — recipe author panel.
    const authorDraft = recipeAuthorDraft();
    if (authorDraft) {
      return (
        <RecipeAuthorPanel
          draft={authorDraft}
          onSaved={async () => {
            setRecipeAuthorDraft(null);
            await db.refresh();
            db.setError("Recipe saved to your library.");
          }}
          onCancel={() => setRecipeAuthorDraft(null)}
        />
      );
    }
    if (showSettings()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">{t("settings.title")}</h2>
            <Show when={connected()}>
              <button onClick={() => setShowSettings(false)} class="colony-close-btn">✕</button>
            </Show>
          </div>
          <SettingsPage onSaved={handleSettingsSaved} />
          <div class="mt-4 border-t border-bark pt-4">
            <h3 class="colony-label mb-2">THEME</h3>
            <select
              value={theme()}
              onChange={(e) => setTheme(e.currentTarget.value)}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
            >
              <option value="colony">Colony (default)</option>
              <option value="springtale">Springtale</option>
            </select>
          </div>
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

    // Per-bot / per-formation AI config (G7)
    if (aiConfigAgent()) {
      const aca = aiConfigAgent()!;
      return (
        <AiConfigPanel
          targetId={aca.id}
          targetName={aca.name}
          scope={aca.scope}
          onSave={async (targetId, config) => {
            const key = aca.scope === "formation"
              ? `ai:formation:${targetId}`
              : `ai:${targetId}`;
            await db.provider.configureAiAdapter(key, config);
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

    // G5e — visual rule builder (global:new_rule command).
    if (showRuleBuilder()) {
      return (
        <RuleBuilderOverlay
          onCancel={() => setShowRuleBuilder(false)}
          onSaved={async () => { await db.refresh(); }}
        />
      );
    }

    // TeamBuilder OOBE — full-panel overlay like settings
    if (showTeamBuilder()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">Build Your Team</h2>
            <button onClick={() => setShowTeamBuilder(false)} class="colony-close-btn">✕</button>
          </div>
          <TeamBuilder
            availableConnectors={availableConnectors()}
            connectors={db.schemas()}
            intents={intents()}
            initialTemplate={teamBuilderSeed() ?? undefined}
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
              setTeamBuilderSeed(null);
              await db.refresh();
              setAvailableConnectors(await db.provider.listAvailableConnectors());
            }}
            onCancel={() => { setShowTeamBuilder(false); setTeamBuilderSeed(null); }}
          />
        </div>
      );
    }

    if (showSessions()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
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
          onHatch={() => setShowModeSelect(true)}
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
          events={db.events()}
          outputs={connectorOutputs() as any}
          availableConnectors={availableConnectors()}
          selection={selection()}
          detailView={detailView()}
          formationCommands={db.formationCommands()}
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
          onCreateBot={() => setShowModeSelect(true)}
        />
      }
      overlay={shellOverlay()}
      notification={notification() ?? undefined}
    />
  );
};
