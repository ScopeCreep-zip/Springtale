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
  ApprovalCard,
  ProofOfLifePanel,
  RecipeAuthorPanel,
  RecipeDeployPanel,
  RecipeLibraryOverlay,
  RecipeQuickView,
  RuleBuilderOverlay,
  SafetyPanel,
  useDashboard,
  useI18n,
  mapNodes,
  mapAgents,
  mapFormations,
} from "@springtale/ui";
import type { ColonySelection, CreateMode, Recipe, RecipeApplyReport, RecipeLibraryVariant, TeamBuilderSeed, TeamConfig } from "@springtale/ui";
import { COMMANDS } from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { getVaultStatus, unlockVault, createVault } from "./ipc/vault";
import { panicWipe } from "./ipc/panic";
import { resetAutoLock } from "./ipc/autolock";
import { TravelModePage } from "./pages/TravelMode";

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
  const [showSafety, setShowSafety] = createSignal(false);
  const [showTravelMode, setShowTravelMode] = createSignal(false);
  const [showRuleBuilder, setShowRuleBuilder] = createSignal(false);
  const [showTeamBuilder, setShowTeamBuilder] = createSignal(false);
  // W1.A — ModeSelectOverlay is the entry hub before any compose flow.
  // Triggered from the empty-canvas hint, top-bar "+ NEW", or `N` key.
  // Per feedback_multi_path_oobe this is a hub, not a linear funnel:
  // each card opens its own flow independently.
  const [showModeSelect, setShowModeSelect] = createSignal(false);
  // F5 — member-picker overlay state. When set to a formation id, the
  // ColonyShell overlay slot renders MemberPickerOverlay for that
  // formation. Cleared on remove or cancel.
  const [memberPickerFor, setMemberPickerFor] = createSignal<string | null>(null);
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

    // G5f — push the persisted disguise state onto the shell on
    // startup so the visible chrome matches the backend from the
    // first frame. A survivor relaunching under duress sees their
    // disguised title immediately, not a brief flash of "Springtale".
    try {
      const { applyDisguiseToShell, applyContentProtection, applyDisguiseToTray, getSafetyConfig } = await import("./ipc/safety");
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
          taps = taps.filter((t) => now - t < PANIC_TAP_WINDOW_MS);
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
        setShowModeSelect(false);
        setAiConfigAgent(null);
        setConnectorConfigData(null);
        return;
      }

      // W2.E — `O` toggles the canvas (OUTPUT) view in the bottom
      // panel. Lets the user reach the A2UI surface without hunting
      // through nested menus.
      if (key === "o" && !e.ctrlKey && !e.metaKey && !e.shiftKey
          && !showVault() && !showDesktopSettings() && !showSafety()
          && !showTravelMode() && !showRuleBuilder() && !showTeamBuilder()
          && !showModeSelect() && recipeLibraryVariant() === null
          && recipeDeploy() === null && proofOfLife() === null
          && !confirmAction() && !aiConfigAgent()
          && !connectorConfigData() && !memberPickerFor()) {
        e.preventDefault();
        setDetailView({ mode: "canvas" });
        return;
      }

      // W1.A — `N` opens the mode-select hub (Nintendo-style entry to
      // every compose flow). Skipped when any other modal is open per
      // the guard below.
      if (key === "n" && !e.ctrlKey && !e.metaKey && !e.shiftKey
          && !showVault() && !showDesktopSettings() && !showSafety()
          && !showTravelMode() && !showRuleBuilder() && !showTeamBuilder()
          && !showModeSelect() && recipeLibraryVariant() === null
          && recipeDeploy() === null
          && !confirmAction() && !aiConfigAgent()
          && !connectorConfigData() && !memberPickerFor()) {
        e.preventDefault();
        setShowModeSelect(true);
        return;
      }

      // Skip command shortcuts when any modal is open
      if (showVault() || showDesktopSettings() || showSafety() || showTravelMode() || showRuleBuilder() || showTeamBuilder() || showModeSelect() || recipeLibraryVariant() !== null || recipeDeploy() !== null || proofOfLife() !== null || recipeAuthorDraft() !== null || confirmAction() || aiConfigAgent() || connectorConfigData() || memberPickerFor()) return;

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

    await listen("vault-locked", () => {
      setVaultLocked(true);
      setShowVault(true);
    });

    await listen("vault-unlocked", async () => {
      setVaultLocked(false);
      setShowVault(false);
      await loadColonyData();
      // G5g — once the runtime is live we can register the
      // persisted global hotkey. Before unlock the in-window
      // listener below is the only handler; after unlock it
      // becomes the fallback for when the window is focused.
      try {
        const { applyQuickHideShortcut } = await import("./ipc/safety");
        await applyQuickHideShortcut();
      } catch (e) {
        db.setError(`quick-hide hotkey registration: ${String(e)}`);
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
      await loadColonyData();
      try {
        const { applyQuickHideShortcut } = await import("./ipc/safety");
        await applyQuickHideShortcut();
      } catch (e) {
        db.setError(`quick-hide hotkey registration: ${String(e)}`);
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

  // ── Data → Colony visual model (real data, no fakes) ───
  const nodes = () => mapNodes(db.connectors());
  const agents = () => mapAgents(db.rules(), db.agentStates());
  const [connections, setConnections] = createSignal<import("@springtale/ui").ColonyConnection[]>([]);
  const formations = () => mapFormations(db.swarms(), db.cooperationEvents());

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
          // G5e — open the visual rule builder. TeamBuilder is the
          // formation-spawning wizard, available via a different
          // entry point; this command targets the rule-composition
          // surface specifically.
          setShowRuleBuilder(true);
          break;
        case "global:make_bot":
          // Canvas is live so a refresh command is redundant — this slot
          // now routes the user back to the bot/team selection hub
          // (ModeSelectOverlay) so they can add another bot after the
          // canvas already has some. Same overlay the cold-boot "NEW"
          // path opens; ModeSelectOverlay branches into RecipeLibraryOverlay
          // (Make a Bot) or TeamBuilder (Make a Team) from there.
          setSelection({ id: null, type: null });
          setShowModeSelect(true);
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
            setAiConfigAgent({ id: sel.id, name: agent?.name ?? sel.id, scope: "agent" });
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
          // The overlay fetches the eligible-removal list (B11
          // `formation_eligible_members`) and renders one button per member.
          if (sel.id) { setMemberPickerFor(sel.id); }
          break;
        case "formation:rally":
          if (sel.id) { await db.handleRallyFormation(sel.id); }
          break;
        case "formation:ai_config":
        case "formation:ai_adapter":
          // G7 — open the per-formation AI override panel. Uses the
          // shared AiConfigPanel scoped to a formation; on save the
          // config persists at `ai:formation:{id}` and the next
          // Fever-tier orchestrate call picks it up via `resolve_ai_config`.
          if (sel.id) {
            const fm = db.swarms().find((s) => s.id === sel.id);
            setAiConfigAgent({ id: sel.id, name: fm?.name ?? sel.id, scope: "formation" });
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

  // W1.A — Mode-select hub handler. Each card opens its own flow.
  // Per `feedback_multi_path_oobe`, the hub doesn't carry state across
  // modes — picking a mode dismisses the hub and opens that mode's
  // entry surface. Returning to the hub re-enters from scratch.
  const [recipeLibraryVariant, setRecipeLibraryVariant] = createSignal<RecipeLibraryVariant | null>(null);
  const handleModeSelect = (mode: CreateMode) => {
    setShowModeSelect(false);
    switch (mode) {
      case "bot":
        setRecipeLibraryVariant("bot");
        break;
      case "team":
        // W1.B's library covers single-bot recipes today; team recipes
        // arrive in a later wave. Open the team-aware variant of the
        // library so the user sees what's there and can fall through
        // to TeamBuilder for from-scratch composition.
        setRecipeLibraryVariant("team");
        break;
      case "addToTeam":
        // Until we ship the "pick a team" picker, open the library so
        // the user can clone a recipe into a new agent.
        setRecipeLibraryVariant("all");
        break;
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
    if (recipeLibraryVariant() === null) return;
    void db.provider
      .listRecipes({ favorites_only: true })
      .then((rs) => setRecipeFavorites(new Set(rs.map((r) => r.id))))
      .catch(() => {});
  });
  // W1.C — recipe deploy panel (the layered required → optional →
  // advanced → code form). When set, the panel replaces the library
  // overlay until the user either deploys or backs out.
  const [recipeDeploy, setRecipeDeploy] = createSignal<Recipe | null>(null);
  // W2.A — seed for TeamBuilder when the user picks CUSTOMIZE on a
  // recipe instead of USE. Reset to `null` after TeamBuilder closes.
  const [teamBuilderSeed, setTeamBuilderSeed] = createSignal<TeamBuilderSeed | null>(null);
  // W2.B — recipe authoring panel. Holds the draft recipe being saved.
  const [recipeAuthorDraft, setRecipeAuthorDraft] = createSignal<Recipe | null>(null);
  // W1.E — post-deploy proof-of-life panel. Set immediately after a
  // successful Deploy so the user sees confirmation + test affordances.
  const [proofOfLife, setProofOfLife] = createSignal<RecipeApplyReport | null>(null);
  // W1.F — pending destructive-action approval request. Set when the
  // backend emits an `approval-required` event; cleared after the
  // user clicks Approve/Deny and the response IPC call completes.
  const [pendingApproval, setPendingApproval] = createSignal<{
    request_id: string;
    connector_name: string;
    action_type: string;
    rationale: string;
  } | null>(null);
  const handleUseRecipe = (recipe: Recipe) => {
    setRecipeLibraryVariant(null);
    setRecipeDeploy(recipe);
  };

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
    // F5 — member-picker for RM MBR. Renders before settings/AI panels
    // so a destructive action gets focus once requested.
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
    // W1.A — ModeSelectOverlay sits below the destructive member-picker
    // but above the rest so first-action discoverability is high.
    if (showModeSelect()) {
      return (
        <ModeSelectOverlay
          hasExistingTeams={(db.swarms()?.length ?? 0) > 0}
          onSelectMode={handleModeSelect}
          onCancel={() => setShowModeSelect(false)}
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
            handleUseRecipe(quick);
          }}
          onCustomize={() => {
            setRecipeQuickView(null);
            setRecipeLibraryVariant(null);
            setTeamBuilderSeed({
              name: quick.name,
              connectorsUsed: quick.connectors_used,
            });
            setShowTeamBuilder(true);
          }}
          onFork={async () => {
            try {
              const forked = await db.provider.forkRecipe(quick.id, `My ${quick.name}`);
              setRecipeQuickView(null);
              setRecipeLibraryVariant(null);
              setRecipeAuthorDraft(forked);
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
    if (recipeLibraryVariant() !== null) {
      return (
        <RecipeLibraryOverlay
          variant={recipeLibraryVariant() ?? "all"}
          favorites={recipeFavorites()}
          onSelectRecipe={(recipe) => setRecipeQuickView(recipe)}
          onToggleFavorite={handleToggleRecipeFavorite}
          onBuildFromScratch={() => {
            setRecipeLibraryVariant(null);
            setTeamBuilderSeed(null);
            setShowTeamBuilder(true);
          }}
          onCancel={() => setRecipeLibraryVariant(null)}
        />
      );
    }
    // W1.C — recipe deploy panel (progressive-disclosure form).
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
    // W1.E — proof-of-life panel runs after Deploy: test trigger,
    // celebration sprite, dismissal.
    const polReport = proofOfLife();
    if (polReport) {
      return (
        <ProofOfLifePanel
          report={polReport}
          onDismiss={() => setProofOfLife(null)}
        />
      );
    }
    // W2.B — recipe author panel (save / fork / build-as-recipe).
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
          onOpenSafety={() => { setShowDesktopSettings(false); setShowSafety(true); }}
          onExportData={async () => { await db.provider.exportData(); }}
          onCompactMemory={async () => { await db.provider.compactMemory(1000); }}
          onClose={() => setShowDesktopSettings(false)}
          theme={theme()}
          onThemeChange={(t) => setTheme(t)}
        />
      );
    }

    // 5b. G5d — safety / disguise panel (reached from Settings → Safety & Disguise…)
    if (showSafety()) {
      return (
        <SafetyPanel
          onClose={() => setShowSafety(false)}
          onPanicWipe={async () => {
            try {
              const { ask } = await import("@tauri-apps/plugin-dialog");
              const ok = await ask("This will irreversibly wipe all data. Are you sure?", { kind: "warning" });
              if (ok) await panicWipe();
            } catch (e) {
              db.setError(String(e));
            }
          }}
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
              const { resetAutoLock } = await import("./ipc/autolock");
              await applyDisguiseToShell();
              await applyDisguiseToTray();
              await applyContentProtection();
              await applyQuickHideShortcut();
              // F — `reset_auto_lock` Tauri command re-reads
              // `auto_lock_minutes` from the persisted safety config
              // and reschedules `AppState.auto_lock` accordingly, so
              // a panel change to the timer takes effect now.
              await resetAutoLock();
            } catch (e) {
              db.setError(String(e));
            }
          }}
          onOpenTravelMode={() => { setShowSafety(false); setShowTravelMode(true); }}
        />
      );
    }

    // 5c. G5h — travel-mode page (encrypted backup + local wipe; §2.6).
    if (showTravelMode()) {
      return (
        <div class="colony-modal mx-auto max-w-2xl overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
          <div class="mb-4 flex items-center justify-between">
            <h2 class="colony-text-md font-bold text-text-primary">Travel mode</h2>
            <button onClick={() => setShowTravelMode(false)} class="colony-close-btn">✕</button>
          </div>
          <TravelModePage />
        </div>
      );
    }

    // 5c. G5e — visual rule builder (global:new_rule command).
    if (showRuleBuilder()) {
      return (
        <RuleBuilderOverlay
          onCancel={() => setShowRuleBuilder(false)}
          onSaved={async () => { await db.refresh(); }}
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
          outputs={connectorOutputs() as any}
          availableConnectors={availableConnectors()}
          events={db.events()}
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
