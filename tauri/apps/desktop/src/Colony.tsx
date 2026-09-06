import {
  AiConfigPanel,
  AppSettingsPanel,
  BottomPanel,
  ChatDock,
  ColonyShell,
  ConnectorConfigPanel,
  createColonyController,
  MemberPickerOverlay,
  ModeSelectOverlay,
  PendingApprovals,
  ProofOfLifePanel,
  panicWipe,
  type Recipe,
  RecipeAuthorPanel,
  RecipeDeployPanel,
  RecipeLibraryOverlay,
  RecipeQuickView,
  RuleBuilderOverlay,
  SafetyPanel,
  TeamBuilder,
  type TeamConfig,
  TopBar,
  useDashboard,
  Viewport,
} from "@springtale/ui";
import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { resetAutoLock } from "./ipc/autolock";
import { lockVault } from "./ipc/vault";
import { TravelModePage } from "./pages/TravelMode";
import { quickHide } from "./safety/quickhide";
import { applySafetyToShell } from "./safety/shell";

/**
 * Springtale Desktop — colony ecosystem dashboard.
 *
 * Layout only. Every shared behaviour — the keyboard handler, the
 * `handleCommand` switch, selection → detail wiring, drag persistence
 * and the theme/locale effects — lives in `createColonyController`,
 * shared with the web dashboard. What differs here is the layout plus
 * the desktop-only safety surfaces: disguise, travel mode, panic-tap,
 * auto-lock and the floating chat dock.
 *
 * Mounted only while the vault is unlocked — `App` owns the gate, and
 * everything here reads the sidecar daemon through `useDashboard()`.
 */
export const Colony = (props: { onLock: () => void }) => {
  const db = useDashboard();

  // ── Desktop-only settings state ─────────────────────────
  const [showDesktopSettings, setShowDesktopSettings] = createSignal(false);
  const [showSafety, setShowSafety] = createSignal(false);
  const [showTravelMode, setShowTravelMode] = createSignal(false);
  // Controlled open state for the chat dock so the command-grid "ASK"
  // action can open it (the dock's own tab toggle still works too).
  const [chatOpen, setChatOpen] = createSignal(false);

  const ctl = createColonyController(db, {
    onOpenSettings: () => setShowDesktopSettings(true),
    onOpenChat: () => setChatOpen(true),
    appOverlayOpen: () => showDesktopSettings() || showSafety() || showTravelMode(),
    onEscape: () => setShowDesktopSettings(false),
    onKeyDown: (e) => {
      // Quick-exit: Ctrl+Shift+Q — instant hide + auto-lock.
      // For IPV survivors who need to hide the app immediately.
      if (e.key.toLowerCase() === "q" && e.ctrlKey && e.shiftKey) {
        e.preventDefault();
        lockVault().catch(() => {});
        // `minimize`, not `hide`: the capability set grants
        // core:window:allow-minimize only (capabilities/default.json).
        quickHide().catch(() => {});
        return true;
      }
      return false;
    },
  });

  // ── Auto-lock (Rust backend) ───────────────────────────
  // The shell no longer reads the safety table, so the interval comes
  // from the config this component fetched over HTTP. 0 = disabled.
  const [autoLockMinutes, setAutoLockMinutes] = createSignal(0);
  const resetTimer = () => {
    const minutes = autoLockMinutes();
    if (minutes <= 0) return;
    resetAutoLock(minutes).catch(() => {});
  };

  onMount(async () => {
    document.addEventListener("mousemove", resetTimer);
    document.addEventListener("keydown", resetTimer);
    document.addEventListener("click", resetTimer);

    await ctl.loadColonyData();

    // G5f/G5g — the daemon owns the safety config; the shell owns its
    // effects. Push them as soon as we can read it, so a survivor sees
    // their disguised title, protected window and hotkey immediately
    // after unlock.
    try {
      const cfg = await db.provider.getSafetyConfig();
      setAutoLockMinutes(cfg.auto_lock_minutes);
      await applySafetyToShell(cfg);

      // G5g — panic-tap gesture detector. Counts rapid clicks on the
      // topmost row of the window (where the title bar sits); at
      // `panic_tap_count` clicks within `PANIC_TAP_WINDOW_MS` the
      // survivor's gesture fires panic-wipe. 0 disables (server-bounded
      // so the gesture can't be lost on the way through 1–10).
      if (cfg.panic_tap_count > 0) {
        const PANIC_TAP_WINDOW_MS = 1500;
        const TITLE_BAR_HEIGHT_PX = 40; // colony shell's top row
        let taps: number[] = [];
        const onTap = (e: MouseEvent) => {
          if (e.clientY > TITLE_BAR_HEIGHT_PX) return;
          const now = performance.now();
          taps = taps.filter((tap) => now - tap < PANIC_TAP_WINDOW_MS);
          taps.push(now);
          if (taps.length >= cfg.panic_tap_count) {
            taps = [];
            // Fire-and-forget; the daemon wipes and exits, so there is
            // no result worth awaiting.
            void panicWipe();
          }
        };
        document.addEventListener("click", onTap, { capture: true });
        onCleanup(() => document.removeEventListener("click", onTap, { capture: true }));
      }
    } catch (e) {
      // Never block the colony on a safety apply — the config is still
      // editable from the Safety panel.
      console.warn("safety config apply skipped:", e);
    }
  });

  onCleanup(() => {
    document.removeEventListener("mousemove", resetTimer);
    document.removeEventListener("keydown", resetTimer);
    document.removeEventListener("click", resetTimer);
  });

  // W1.B-quickview — `RecipeQuickView` is mounted here (not by
  // RecipeLibraryOverlay internally) so the library stays
  // structurally flat (Show siblings only, mirroring
  // MemberPickerOverlay). When set, QuickView renders on top of the
  // library; cancelling returns to the library.
  const [recipeQuickView, setRecipeQuickView] = createSignal<Recipe | null>(null);
  // Favorites Set lives at this scope so both the library grid and
  // the quick-view modal see the same state. Backend (config_store)
  // is the source of truth; this Set is a render-only mirror seeded
  // on first dashboard refresh + updated when the user toggles.
  const [recipeFavorites, setRecipeFavorites] = createSignal<Set<string>>(new Set());
  const handleToggleRecipeFavorite = async (recipe: Recipe) => {
    try {
      const nowFav = await db.provider.toggleRecipeFavorite(recipe.id);
      setRecipeFavorites((prev) => {
        const next = new Set(prev);
        if (nowFav) next.add(recipe.id);
        else next.delete(recipe.id);
        return next;
      });
    } catch (e) {
      db.setError(String(e));
    }
  };
  // Seed favorites from the backend config_store the first time the
  // user opens the library after unlock. The library reads
  // `recipeFavorites()` so the cards render the heart correctly for
  // recipes the user has previously starred.
  createEffect(() => {
    if (ctl.recipeLibraryVariant() === null) return;
    void db.provider
      .listRecipes({ favorites_only: true })
      .then((rs) => setRecipeFavorites(new Set(rs.map((r) => r.id))))
      .catch(() => {});
  });
  const confirmPanicWipe = async () => {
    try {
      const { ask } = await import("@tauri-apps/plugin-dialog");
      const ok = await ask("This will irreversibly wipe all data. Are you sure?", {
        kind: "warning",
      });
      if (ok) await panicWipe();
    } catch (e) {
      db.setError(String(e));
    }
  };

  // ── Unified overlay — priority order ────────────────────
  const shellOverlay = () => {
    // Plan 6.7 — the approval queue (`GET /approvals`), refreshed by the
    // daemon's `approval_required` stream event. Same panel the web
    // dashboard mounts; the sentinel prompt arrives the same way.
    if (db.pendingApprovals().length > 0) {
      return <PendingApprovals />;
    }
    // F5 — member-picker for RM MBR. Renders before settings/AI panels
    // so a destructive action gets focus once requested.
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
    // W1.A — ModeSelectOverlay sits below the destructive member-picker
    // but above the rest so first-action discoverability is high.
    if (ctl.showModeSelect()) {
      return (
        <ModeSelectOverlay
          hasExistingTeams={(db.swarms()?.length ?? 0) > 0}
          onSelectMode={ctl.handleModeSelect}
          onCancel={() => ctl.setShowModeSelect(false)}
        />
      );
    }
    // W1.B-quickview — RecipeQuickView lives at App scope (not
    // inside the library) so the library JSX stays structurally
    // flat. Priority: above the library so clicking a card opens
    // the modal *on top* of the library; closing it returns to the
    // library since `recipeLibraryVariant()` is still set.
    const quick = recipeQuickView();
    if (quick) {
      return (
        <RecipeQuickView
          recipe={quick}
          isFavorite={recipeFavorites().has(quick.id)}
          onUse={() => {
            setRecipeQuickView(null);
            ctl.handleUseRecipe(quick);
          }}
          onCustomize={() => {
            setRecipeQuickView(null);
            ctl.setRecipeLibraryVariant(null);
            ctl.setTeamBuilderSeed({
              name: quick.name,
              connectorsUsed: quick.connectors_used,
            });
            ctl.setShowTeamBuilder(true);
          }}
          onFork={async () => {
            try {
              const forked = await db.provider.forkRecipe(quick.id, `My ${quick.name}`);
              setRecipeQuickView(null);
              ctl.setRecipeLibraryVariant(null);
              ctl.setRecipeAuthorDraft(forked);
            } catch (e) {
              db.setError(String(e));
            }
          }}
          onToggleFavorite={() => handleToggleRecipeFavorite(quick)}
          onCancel={() => setRecipeQuickView(null)}
        />
      );
    }
    // W1.B — recipe library opens from any mode-select card. Closes
    // back to the mode-select hub when dismissed; "Build from scratch"
    // falls through to TeamBuilder.
    if (ctl.recipeLibraryVariant() !== null) {
      return (
        <RecipeLibraryOverlay
          variant={ctl.recipeLibraryVariant() ?? "all"}
          favorites={recipeFavorites()}
          onSelectRecipe={(recipe) => setRecipeQuickView(recipe)}
          onToggleFavorite={handleToggleRecipeFavorite}
          onBuildFromScratch={() => {
            ctl.setRecipeLibraryVariant(null);
            ctl.setTeamBuilderSeed(null);
            ctl.setShowTeamBuilder(true);
          }}
          onCancel={() => ctl.setRecipeLibraryVariant(null)}
        />
      );
    }
    // W1.C — recipe deploy panel (progressive-disclosure form).
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
    // W1.E — proof-of-life panel runs after Deploy: test trigger,
    // celebration sprite, dismissal.
    const polReport = ctl.proofOfLife();
    if (polReport) {
      return <ProofOfLifePanel report={polReport} onDismiss={() => ctl.setProofOfLife(null)} />;
    }
    // W2.B — recipe author panel (save / fork / build-as-recipe).
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
    // 1. Confirm dialog (destructive actions)
    const ca = ctl.confirmAction();
    if (ca) {
      return (
        <div class="mx-auto max-w-lg rounded border-2 border-bark bg-soil-mid p-6 text-center">
          <p class="colony-text-md font-bold text-text-primary">{ca.title}</p>
          <p class="colony-text-xs mt-2 text-text-secondary">{ca.message}</p>
          <div class="mt-4 flex justify-center gap-3">
            <button
              type="button"
              class="colony-command-btn colony-text-2xs px-4 py-2"
              data-tone="error"
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

    // 2. Per-bot AI config (agent:ai_config command)
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

    // 3. Connector config — full management panel
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

    // 4. App settings
    if (showDesktopSettings()) {
      return (
        <AppSettingsPanel
          isDesktop={true}
          onVault={() => {
            setShowDesktopSettings(false);
            props.onLock();
          }}
          onPanicWipe={confirmPanicWipe}
          onOpenSafety={() => {
            setShowDesktopSettings(false);
            setShowSafety(true);
          }}
          onExportData={async () => {
            await db.provider.exportData();
          }}
          onCompactMemory={async () => {
            await db.provider.compactMemory(1000);
          }}
          onClose={() => setShowDesktopSettings(false)}
          theme={ctl.theme()}
          onThemeChange={(next) => ctl.setTheme(next)}
        />
      );
    }

    // 4b. G5d — safety / disguise panel (reached from Settings → Safety & Disguise…)
    if (showSafety()) {
      return (
        <SafetyPanel
          onClose={() => setShowSafety(false)}
          onPanicWipe={confirmPanicWipe}
          onSafetyChanged={async () => {
            // F — every panel save (disguise focused-updates AND the
            // full-form Save button) routes here so the running
            // process picks up the new safety state without a
            // restart. Without this, toggles persisted in the daemon
            // but the window title, content protection, global
            // shortcut and auto-lock timer kept their old values until
            // next boot — which is what made the panel feel inert.
            try {
              const cfg = await db.provider.getSafetyConfig();
              setAutoLockMinutes(cfg.auto_lock_minutes);
              await applySafetyToShell(cfg);
            } catch (e) {
              db.setError(String(e));
            }
          }}
          onOpenTravelMode={() => {
            setShowSafety(false);
            setShowTravelMode(true);
          }}
        />
      );
    }

    // 4c. G5h — travel-mode page (encrypted backup + local wipe; §2.6).
    if (showTravelMode()) {
      return (
        <div class="colony-modal mx-auto max-w-2xl overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">Travel mode</h2>
            <button type="button" onClick={() => setShowTravelMode(false)} class="colony-close-btn">
              ✕
            </button>
          </div>
          <TravelModePage />
        </div>
      );
    }

    // 4d. G5e — visual rule builder (global:new_rule command).
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

    // 5. TeamBuilder OOBE — full-panel overlay like settings
    if (ctl.showTeamBuilder()) {
      return (
        <div class="colony-modal mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
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
        <div class="relative h-full w-full">
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
          />
          {/* Floating chat dock — bottom-left, above the minimap. */}
          <ChatDock open={chatOpen()} onOpenChange={setChatOpen} />
        </div>
      }
      bottomPanel={
        <BottomPanel
          nodes={ctl.nodes()}
          agents={ctl.agents()}
          connections={ctl.connections()}
          formations={ctl.formations()}
          connectorPositions={ctl.connectorPositions()}
          outputs={ctl.connectorOutputs()}
          availableConnectors={ctl.availableConnectors()}
          events={db.events()}
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
