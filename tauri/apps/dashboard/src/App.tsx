import {
  AiConfigPanel,
  BottomPanel,
  ChatPanel,
  ColonyShell,
  ConnectorConfigPanel,
  createColonyController,
  MemberPickerOverlay,
  ModeSelectOverlay,
  PendingApprovals,
  ProofOfLifePanel,
  RecipeAuthorPanel,
  RecipeDeployPanel,
  RecipeLibraryOverlay,
  RuleBuilderOverlay,
  TeamBuilder,
  type TeamConfig,
  TopBar,
  useDashboard,
  useI18n,
  Viewport,
} from "@springtale/ui";
import { createSignal, Show } from "solid-js";
import { SessionsPage } from "./pages/Sessions";
import { SettingsPage } from "./pages/Settings";

/**
 * Springtale Web Dashboard — colony ecosystem view.
 *
 * Layout only. Every shared behaviour — the keyboard handler, the
 * `handleCommand` switch, selection → detail wiring, drag persistence
 * and the theme/locale effects — lives in `createColonyController`,
 * shared with the desktop shell. What differs here is the layout:
 * settings and sessions render as full-screen overlays, and chat is a
 * modal rather than a floating dock.
 */
export const App = () => {
  const db = useDashboard();
  const { t } = useI18n();

  // ── Web-only overlay state ──────────────────────────────
  const [showSettings, setShowSettings] = createSignal(false);
  const [showSessions, setShowSessions] = createSignal(false);
  const [showChat, setShowChat] = createSignal(false);
  const [connected, setConnected] = createSignal(false);

  const ctl = createColonyController(db, {
    onOpenSettings: () => {
      setShowSettings(true);
      setShowSessions(false);
    },
    onOpenChat: () => setShowChat(true),
    appOverlayOpen: () => showSettings() || showSessions() || showChat(),
    onEscape: () => {
      setShowSettings(false);
      setShowSessions(false);
      setShowChat(false);
    },
  });

  const handleSettingsSaved = async () => {
    setConnected(true);
    await ctl.loadColonyData();
  };

  // ── Unified overlay — settings → confirm → hatch → sessions ──
  const shellOverlay = () => {
    // Plan 6.7 — a pending approval gets focus over everything else.
    if (db.pendingApprovals().length > 0) {
      return <PendingApprovals />;
    }
    // F5 — member picker takes precedence so destructive flow gets focus.
    const pickerFor = ctl.memberPickerFor();
    if (pickerFor) {
      return (
        <MemberPickerOverlay
          formationId={pickerFor}
          onRemoved={async () => {
            await db.refresh();
          }}
          onCancel={() => ctl.setMemberPickerFor(null)}
        />
      );
    }
    // W1.A — Mode select hub before any compose flow.
    if (ctl.showModeSelect()) {
      return (
        <ModeSelectOverlay
          hasExistingTeams={(db.formations()?.length ?? 0) > 0}
          onSelectMode={ctl.handleModeSelect}
          onCancel={() => ctl.setShowModeSelect(false)}
        />
      );
    }
    // W1.B — Recipe library opens from any mode-select card.
    if (ctl.recipeLibraryVariant() !== null) {
      return (
        <RecipeLibraryOverlay
          variant={ctl.recipeLibraryVariant() ?? "all"}
          favorites={new Set<string>()}
          onSelectRecipe={ctl.handleUseRecipe}
          onToggleFavorite={() => {}}
          onBuildFromScratch={() => {
            ctl.setRecipeLibraryVariant(null);
            ctl.setTeamBuilderSeed(null);
            ctl.setShowTeamBuilder(true);
          }}
          onCancel={() => ctl.setRecipeLibraryVariant(null)}
        />
      );
    }
    // W1.C — recipe deploy panel.
    const deployRecipe = ctl.recipeDeploy();
    if (deployRecipe) {
      return (
        <RecipeDeployPanel
          recipe={deployRecipe}
          onDeployed={async (report) => {
            ctl.setRecipeDeploy(null);
            await db.refresh();
            ctl.setProofOfLife(report);
          }}
          onCancel={() => ctl.setRecipeDeploy(null)}
        />
      );
    }
    // W1.E — proof-of-life panel.
    const polReport = ctl.proofOfLife();
    if (polReport) {
      return <ProofOfLifePanel report={polReport} onDismiss={() => ctl.setProofOfLife(null)} />;
    }
    // W2.B — recipe author panel.
    const authorDraft = ctl.recipeAuthorDraft();
    if (authorDraft) {
      return (
        <RecipeAuthorPanel
          draft={authorDraft}
          onSaved={async () => {
            ctl.setRecipeAuthorDraft(null);
            await db.refresh();
            db.setError("Recipe saved to your library.");
          }}
          onCancel={() => ctl.setRecipeAuthorDraft(null)}
        />
      );
    }
    if (showSettings()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">{t("settings.title")}</h2>
            <Show when={connected()}>
              <button type="button" onClick={() => setShowSettings(false)} class="colony-close-btn">
                ✕
              </button>
            </Show>
          </div>
          <SettingsPage onSaved={handleSettingsSaved} />
          <div class="mt-4 border-t border-bark pt-4">
            <h3 class="colony-label mb-2">THEME</h3>
            <select
              value={ctl.theme()}
              onChange={(e) => ctl.setTheme(e.currentTarget.value)}
              class="colony-text-2xs w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
            >
              <option value="springtale">Springtale (default)</option>
              <option value="colony">Colony</option>
            </select>
          </div>
        </div>
      );
    }

    const ca = ctl.confirmAction();
    if (ca) {
      return (
        <div class="mx-auto max-w-lg rounded border-2 border-bark bg-soil-mid p-6 text-center">
          <p class="colony-text-md font-bold text-text-primary">{ca.title}</p>
          <p class="colony-text-xs mt-2 text-text-secondary">{ca.message}</p>
          <div class="mt-4 flex justify-center gap-3">
            <button
              type="button"
              class="colony-command-btn colony-command-btn--danger colony-text-2xs px-4 py-2"
              onClick={async () => {
                try {
                  await ca.action();
                  ctl.setConfirmAction(null);
                } catch (e) {
                  db.setError(String(e));
                  ctl.setConfirmAction(null);
                }
              }}
            >
              {ca.label}
            </button>
            <button
              type="button"
              class="colony-command-btn colony-text-2xs px-4 py-2"
              onClick={() => ctl.setConfirmAction(null)}
            >
              Cancel
            </button>
          </div>
        </div>
      );
    }

    // Per-bot / per-formation AI config (G7)
    const aca = ctl.aiConfigAgent();
    if (aca) {
      return (
        <AiConfigPanel
          targetId={aca.id}
          targetName={aca.name}
          scope={aca.scope}
          onSave={async (targetId, config) => {
            const target =
              aca.scope === "formation"
                ? ({ scope: "formation", id: targetId } as const)
                : ({ scope: "agent", rule_id: targetId } as const);
            await db.provider.configureAiAdapter(target, config);
            await db.refresh();
          }}
          onClose={() => ctl.setAiConfigAgent(null)}
        />
      );
    }

    // Connector config — full management panel
    const ccd = ctl.connectorConfigData();
    if (ccd) {
      return (
        <ConnectorConfigPanel
          connectorId={ccd.id}
          schemas={db.schemas()}
          conditionTypes={ctl.conditionTypes()}
          rules={db.rules().filter((r) => (r.connector ?? r.triggerType) === ccd.id)}
          currentConfig={ccd.config}
          configSchema={ccd.configSchema}
          onSave={async (id, config) => {
            await db.provider.upsertConnectorConfig(id, config);
            ctl.setAvailableConnectors(await db.provider.listAvailableConnectors());
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
          onClose={() => ctl.setConnectorConfigData(null)}
        />
      );
    }

    // G5e — visual rule builder (global:new_rule command).
    if (ctl.showRuleBuilder()) {
      return (
        <RuleBuilderOverlay
          onCancel={() => ctl.setShowRuleBuilder(false)}
          onSaved={async () => {
            await db.refresh();
          }}
        />
      );
    }

    // TeamBuilder OOBE — full-panel overlay like settings
    if (ctl.showTeamBuilder()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">Build Your Team</h2>
            <button
              type="button"
              onClick={() => ctl.setShowTeamBuilder(false)}
              class="colony-close-btn"
            >
              ✕
            </button>
          </div>
          <TeamBuilder
            availableConnectors={ctl.availableConnectors()}
            connectors={db.schemas()}
            intents={ctl.intents()}
            initialTemplate={ctl.teamBuilderSeed() ?? undefined}
            onSetupConnector={ctl.setupConnector}
            onParseRule={async (intent) => db.provider.parseRuleFromIntent(intent)}
            onSaveAiConfig={async (config) => {
              await db.provider.configureAiAdapter({ scope: "colony" }, config);
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
              ctl.setShowTeamBuilder(false);
              ctl.setTeamBuilderSeed(null);
              await db.refresh();
              ctl.setAvailableConnectors(await db.provider.listAvailableConnectors());
            }}
            onCancel={() => {
              ctl.setShowTeamBuilder(false);
              ctl.setTeamBuilderSeed(null);
            }}
          />
        </div>
      );
    }

    if (showSessions()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">{t("sessions.title")}</h2>
            <button type="button" onClick={() => setShowSessions(false)} class="colony-close-btn">
              ✕
            </button>
          </div>
          <SessionsPage />
        </div>
      );
    }

    if (showChat()) {
      return (
        <div class="colony-modal mx-auto flex h-[80vh] max-h-[80vh] w-full max-w-lg flex-col overflow-hidden rounded border-2 border-bark bg-soil-mid">
          <div class="flex items-center justify-between border-b border-soil-line p-3">
            <h2 class="colony-text-md font-bold text-text-primary">Ask Springtale</h2>
            <button type="button" onClick={() => setShowChat(false)} class="colony-close-btn">
              ✕
            </button>
          </div>
          <div class="min-h-0 flex-1">
            <ChatPanel />
          </div>
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
          agents={ctl.agents()}
          nodes={ctl.nodes()}
          formations={ctl.formations()}
          events={db.events()}
          selection={ctl.selection()}
          onSelectAgent={ctl.selectAgent}
          onSelectFormation={ctl.selectFormation}
        />
      }
      viewport={
        <Viewport
          nodes={ctl.nodes()}
          agents={ctl.agents()}
          connections={ctl.connections()}
          formations={ctl.formations()}
          utterances={db.utterances()}
          colonyNow={db.colonyNow()}
          agentToConnector={db.agentToConnector()}
          framesFor={db.framesFor}
          roleOf={db.roleOf}
          events={db.events()}
          selection={ctl.selection()}
          onSelectConnector={ctl.selectConnector}
          onSelectAgent={ctl.selectAgent}
          onSelectFormation={ctl.selectFormation}
          onClearSelection={ctl.clearSelection}
          connectorPositions={ctl.connectorPositions()}
          onConnectorDrag={ctl.handleConnectorDrag}
          onHatch={() => ctl.setShowModeSelect(true)}
          availableConnectors={ctl.availableConnectors()}
          connectorSchemas={db.schemas()}
          onSetupConnector={ctl.setupConnector}
          onParseRule={async (intent) => db.provider.parseRuleFromIntent(intent)}
          canvasOverlay={ctl.overlay()}
        />
      }
      bottomPanel={
        <BottomPanel
          nodes={ctl.nodes()}
          agents={ctl.agents()}
          connections={ctl.connections()}
          formations={ctl.formations()}
          connectorPositions={ctl.connectorPositions()}
          events={db.events()}
          outputs={ctl.connectorOutputs()}
          availableConnectors={ctl.availableConnectors()}
          selection={ctl.selection()}
          detailView={ctl.detailView()}
          formationCommands={db.formationCommands()}
          onCommand={ctl.handleCommand}
          onSelectAgent={ctl.selectAgent}
          onSelectConnector={ctl.handleSelectConnectorFromPanel}
          onAddToFormation={ctl.handleAddToFormation}
          onSetupConnector={ctl.setupConnector}
          onCreateBot={() => ctl.setShowModeSelect(true)}
        />
      }
      overlay={shellOverlay()}
      notification={ctl.notification() ?? undefined}
    />
  );
};
