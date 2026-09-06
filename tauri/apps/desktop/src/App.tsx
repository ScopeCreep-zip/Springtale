import {
  AiConfigPanel,
  ApprovalCard,
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
  useI18n,
  Viewport,
} from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { resetAutoLock } from "./ipc/autolock";
import { panicWipe } from "./ipc/panic";
import { createVault, getVaultStatus, unlockVault } from "./ipc/vault";
import { TravelModePage } from "./pages/TravelMode";

/**
 * Springtale Desktop — colony ecosystem dashboard.
 *
 * Layout only. Every shared behaviour — the keyboard handler, the
 * `handleCommand` switch, selection → detail wiring, drag persistence
 * and the theme/locale effects — lives in `createColonyController`,
 * shared with the web dashboard. What differs here is the layout plus
 * the desktop-only safety surfaces: vault, disguise, travel mode,
 * panic-tap, auto-lock and the floating chat dock.
 */
export const App = () => {
  const db = useDashboard();
  const { t } = useI18n();

  // ── Desktop-only: vault + settings state ────────────────
  const [vaultLocked, setVaultLocked] = createSignal(true);
  const [showVault, setShowVault] = createSignal(false);
  const [showDesktopSettings, setShowDesktopSettings] = createSignal(false);
  const [showSafety, setShowSafety] = createSignal(false);
  const [showTravelMode, setShowTravelMode] = createSignal(false);
  // Controlled open state for the chat dock so the command-grid "ASK"
  // action can open it (the dock's own tab toggle still works too).
  const [chatOpen, setChatOpen] = createSignal(false);
  const [passphrase, setPassphrase] = createSignal("");
  const [vaultError, setVaultError] = createSignal("");

  const ctl = createColonyController(db, {
    onOpenSettings: () => setShowDesktopSettings(true),
    onOpenChat: () => setChatOpen(true),
    appOverlayOpen: () => showVault() || showDesktopSettings() || showSafety() || showTravelMode(),
    onEscape: () => setShowDesktopSettings(false),
    onKeyDown: (e) => {
      // Quick-exit: Ctrl+Shift+Q — instant hide + auto-lock.
      // For IPV survivors who need to hide the app immediately.
      if (e.key.toLowerCase() === "q" && e.ctrlKey && e.shiftKey) {
        e.preventDefault();
        invoke("lock_vault").catch(() => {});
        invoke("plugin:window|hide").catch(() => {});
        return true;
      }
      return false;
    },
  });

  // ── Auto-lock (Rust backend) ───────────────────────────
  const resetTimer = () => {
    resetAutoLock().catch(() => {});
  };

  onMount(async () => {
    document.addEventListener("mousemove", resetTimer);
    document.addEventListener("keydown", resetTimer);
    document.addEventListener("click", resetTimer);
    resetTimer();

    // G5f — push the persisted disguise state onto the shell on
    // startup so the visible chrome matches the backend from the
    // first frame. A survivor relaunching under duress sees their
    // disguised title immediately, not a brief flash of "Springtale".
    try {
      const { applyDisguiseToShell, applyContentProtection, applyDisguiseToTray, getSafetyConfig } =
        await import("./ipc/safety");
      await applyDisguiseToShell();
      // G5f — swap the tray icon + tooltip to the disguise profile
      // so the survivor's system-tray surface matches the window
      // chrome. Soft-fails on Linux WMs that don't expose tray.
      await applyDisguiseToTray();
      // G5g — apply screen-recording protection so screenshots
      // taken in a coercive setting return a black frame on
      // macOS + Windows.
      await applyContentProtection();

      // G5g — panic-tap gesture detector. Counts rapid clicks on
      // the topmost row of the window (where the title bar sits);
      // at `panic_tap_count` clicks within `PANIC_TAP_WINDOW_MS`
      // the survivor's gesture fires panic-wipe. `panic_tap_count`
      // = 0 disables (server-bounded so the gesture can't be
      // disabled by accident on the way through 1–10).
      const cfg = await getSafetyConfig();
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
            // Fire-and-forget; the wipe op exits the process on
            // success, so we don't need to await the result.
            void panicWipe();
          }
        };
        document.addEventListener("click", onTap, { capture: true });
        onCleanup(() => document.removeEventListener("click", onTap, { capture: true }));
      }
    } catch {
      // First-run / pre-init — the apply call needs a live runtime.
      // Re-applied after vault unlock in the auto-lock flow.
    }

    await listen("vault-locked", () => {
      setVaultLocked(true);
      setShowVault(true);
    });

    await listen("vault-unlocked", async () => {
      setVaultLocked(false);
      setShowVault(false);
      await ctl.loadColonyData();
      // G5g — once the runtime is live we can register the
      // persisted global hotkey. Before unlock the in-window
      // listener is the only handler; after unlock it becomes the
      // fallback for when the window is focused.
      // Best-effort: the backend tries the configured combo then fallbacks
      // and never errors on conflict (a global shortcut is a convenience, not
      // a requirement). The in-window listener covers focus regardless, so a
      // registration miss must NEVER raise a blocking banner.
      try {
        const { applyQuickHideShortcut } = await import("./ipc/safety");
        await applyQuickHideShortcut();
      } catch (e) {
        console.warn("quick-hide global hotkey registration skipped:", e);
      }
    });

    // G5g — backend fires "quick-hide" when the global hotkey is
    // pressed from anywhere on the desktop. Mirror the in-window
    // path: lock the vault (window is already hidden by the Rust
    // handler so we don't need to hide here).
    await listen("quick-hide", () => {
      invoke("lock_vault").catch((e) => db.setError(String(e)));
    });

    // W1.F — destructive-action approval prompt. Backend emits this
    // when the sentinel hits `ActionImpact::Destructive`; we render
    // an ApprovalCard overlay and forward the user's decision back.
    await listen<{
      request_id: string;
      connector_name: string;
      action_type: string;
      rationale: string;
    }>("approval-required", (event) => {
      setPendingApproval(event.payload);
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
      await ctl.loadColonyData();
      try {
        const { applyQuickHideShortcut } = await import("./ipc/safety");
        await applyQuickHideShortcut();
      } catch (e) {
        // Non-fatal — never block the UI for a global-shortcut miss.
        console.warn("quick-hide global hotkey registration skipped:", e);
      }
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

  // W1.B-quickview — `RecipeQuickView` is mounted by App.tsx (not
  // by RecipeLibraryOverlay internally) so the library stays
  // structurally flat (Show siblings only, mirroring
  // MemberPickerOverlay). When set, App.tsx renders QuickView on
  // top of the library; cancelling returns to the library.
  const [recipeQuickView, setRecipeQuickView] = createSignal<Recipe | null>(null);
  // Favorites Set lives at App scope so both the library grid and
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
  // W1.F — pending destructive-action approval request. Set when the
  // backend emits an `approval-required` event; cleared after the
  // user clicks Approve/Deny and the response IPC call completes.
  const [pendingApproval, setPendingApproval] = createSignal<{
    request_id: string;
    connector_name: string;
    action_type: string;
    rationale: string;
  } | null>(null);

  // W1.F — approval response handler. Forwards the user's decision
  // back to the backend dispatcher, then clears the pending state so
  // the next request (or a re-prompt) can render.
  const handleApproval = async (approve: boolean) => {
    const pending = pendingApproval();
    if (!pending) return;
    try {
      const { respondToApproval } = await import("./ipc/approval");
      await respondToApproval(pending.request_id, approve);
    } catch (e) {
      db.setError(`approval response failed: ${String(e)}`);
    } finally {
      setPendingApproval(null);
    }
  };

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
    // W1.F — destructive-action approval takes top priority: the
    // sentinel is blocked on the user's answer, so we render this
    // above everything else.
    const approval = pendingApproval();
    if (approval) {
      return (
        <ApprovalCard
          connectorName={approval.connector_name}
          actionType={approval.action_type}
          rationale={approval.rationale}
          onDecision={handleApproval}
        />
      );
    }
    // Plan 6.7 — the runtime chat gate's queue (`list_pending_approvals`),
    // same panel the dashboard mounts.
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
    // 1. Vault (security — blocks everything else)
    if (showVault()) {
      return (
        <div class="colony-modal mx-auto max-w-lg space-y-5 overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <h2 class="colony-text-md font-bold text-text-primary">{t("vault.title")}</h2>
          <p class="colony-text-xs text-text-dim">{vaultLocked() ? t("vault.createDesc") : ""}</p>
          {vaultError() && (
            <div class="colony-text-2xs border border-status-error bg-status-error/10 p-2 text-status-error">
              {vaultError()}
            </div>
          )}
          <div>
            <label for="vault-pass" class="colony-text-2xs text-text-secondary">
              {t("vault.passphrase")}
            </label>
            <input
              id="vault-pass"
              type="password"
              value={passphrase()}
              onInput={(e) => setPassphrase(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleUnlock();
              }}
              class="colony-text-xs mt-2 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
            />
          </div>
          <div class="flex gap-3">
            <button
              type="button"
              onClick={handleUnlock}
              class="colony-text-2xs border-2 border-status-ok bg-soil-light px-4 py-2 text-status-ok hover:bg-soil-deep"
            >
              {t("vault.unlock")}
            </button>
            <button
              type="button"
              onClick={handleCreateVault}
              class="colony-text-2xs border-2 border-bark bg-soil-light px-4 py-2 text-text-secondary hover:bg-soil-deep"
            >
              {t("vault.create")}
            </button>
            <Show when={!vaultLocked()}>
              <button type="button" onClick={() => setShowVault(false)} class="colony-close-btn">
                ✕
              </button>
            </Show>
          </div>
        </div>
      );
    }

    // 2. Confirm dialog (destructive actions)
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

    // 3. Per-bot AI config (agent:ai_config command)
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

    // 4. Connector config — full management panel
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

    // 5. App settings
    if (showDesktopSettings()) {
      return (
        <AppSettingsPanel
          isDesktop={true}
          onVault={() => {
            setShowDesktopSettings(false);
            setShowVault(true);
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

    // 5b. G5d — safety / disguise panel (reached from Settings → Safety & Disguise…)
    if (showSafety()) {
      return (
        <SafetyPanel
          onClose={() => setShowSafety(false)}
          onPanicWipe={confirmPanicWipe}
          onSafetyChanged={async () => {
            // F — every panel save (disguise focused-updates AND the
            // full-form Save button) routes here so the running
            // process picks up the new safety state without a
            // restart. Without this, toggles persisted to SQLite but
            // the window title, content protection, global shortcut,
            // and auto-lock timer kept their old values until next
            // boot — which is what made the panel feel inert.
            try {
              const {
                applyDisguiseToShell,
                applyContentProtection,
                applyDisguiseToTray,
                applyQuickHideShortcut,
              } = await import("./ipc/safety");
              const { resetAutoLock: resetAutoLockNow } = await import("./ipc/autolock");
              await applyDisguiseToShell();
              await applyDisguiseToTray();
              await applyContentProtection();
              await applyQuickHideShortcut();
              // F — `reset_auto_lock` Tauri command re-reads
              // `auto_lock_minutes` from the persisted safety config
              // and reschedules `AppState.auto_lock` accordingly, so
              // a panel change to the timer takes effect now.
              await resetAutoLockNow();
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

    // 5c. G5h — travel-mode page (encrypted backup + local wipe; §2.6).
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

    // 5c. G5e — visual rule builder (global:new_rule command).
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

    // 6. TeamBuilder OOBE — full-panel overlay like settings
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
