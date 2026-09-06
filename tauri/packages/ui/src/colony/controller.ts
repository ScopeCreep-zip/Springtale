import type { AvailableConnector, ConfigSchema } from "@springtale/types";
import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { useDashboard } from "../dashboard/context";
import type { ConnectorOutput, Recipe, RecipeApplyReport } from "../dashboard/types";
import { useI18n } from "../i18n/context";
import type { CreateMode } from "./ModeSelectOverlay";
import { mapAgents, mapFormations, mapNodes } from "./mappers";
import type { OverlayMode } from "./overlay";
import { nextOverlay } from "./overlay";
import type { RecipeLibraryVariant } from "./RecipeLibraryOverlay";
import type { TeamBuilderSeed } from "./TeamBuilder";
import type { ColonyConnection, ColonySelection, DetailView } from "./types";
import { COMMANDS } from "./types";

/** The dashboard state object both apps build with `useDashboard()`. */
export type ColonyDb = ReturnType<typeof useDashboard>;

/** A destructive action awaiting the user's confirmation. */
export interface ConfirmAction {
  title: string;
  message: string;
  label: string;
  action: () => Promise<void>;
}

/** Target of the AI-config panel — a single bot, or a whole formation. */
export interface AiConfigTarget {
  id: string;
  name: string;
  scope: "agent" | "formation";
}

/** Connector whose configuration panel is open. */
export interface ConnectorConfigData {
  id: string;
  config: unknown;
  configSchema?: ConfigSchema;
}

/** Transient toast shown by `ColonyShell`. */
export interface ColonyNotification {
  message: string;
  type: "ok" | "warn";
}

/**
 * Layout-specific hooks. Everything the two shells genuinely disagree
 * about lives here; everything else is shared by construction.
 */
export interface ColonyControllerOptions {
  /** `global:settings` — desktop opens `AppSettingsPanel`, web opens its settings overlay. */
  onOpenSettings: () => void;
  /** `global:chat` — desktop opens the floating dock, web opens the chat overlay. */
  onOpenChat: () => void;
  /**
   * True while an app-owned overlay is open (vault, safety, travel mode,
   * sessions…). Shared command hotkeys stay inert while it is.
   */
  appOverlayOpen?: () => boolean;
  /** Escape — close app-owned overlays. Shared overlays are closed for you. */
  onEscape?: () => void;
  /**
   * App-owned key bindings, tried before the shared ones (the desktop's
   * Ctrl+Shift+Q quick-exit). Return `true` to consume the event.
   */
  onKeyDown?: (e: KeyboardEvent) => boolean;
}

/**
 * The one colony controller.
 *
 * Owns the keyboard handler, the `handleCommand` switch, selection →
 * detail-view wiring, drag persistence, and the theme + locale effects
 * — the parts the desktop shell and the web dashboard used to keep two
 * drifting copies of. Both `App.tsx` files are now layout plus
 * `const ctl = createColonyController(db)`.
 *
 * Must be called from a component body: it registers Solid effects,
 * an `onMount` keydown listener, and its matching `onCleanup`.
 */
export function createColonyController(db: ColonyDb, opts: ColonyControllerOptions) {
  const { locale, setLocale } = useI18n();

  // ── Theme ────────────────────────────────────────────────
  const [theme, setTheme] = createSignal("springtale");
  const applyTheme = (t: string) => {
    document.documentElement.dataset.theme = t;
  };

  // ── Colony state ────────────────────────────────────────
  const [selection, setSelection] = createSignal<ColonySelection>({ id: null, type: null });
  // Plan 3.6 — canvas overlay mode, cycled by `O`. Lives here (not in a
  // component) so the canvas, the chip and the hotkey all read one signal.
  const [overlay, setOverlay] = createSignal<OverlayMode>("none");
  const [detailView, setDetailView] = createSignal<DetailView>({ mode: "colony" });
  const [connectorPositions, setConnectorPositions] = createSignal<
    Record<string, { x: number; y: number }>
  >({});
  const [connections, setConnections] = createSignal<ColonyConnection[]>([]);
  const [confirmAction, setConfirmAction] = createSignal<ConfirmAction | null>(null);
  const [aiConfigAgent, setAiConfigAgent] = createSignal<AiConfigTarget | null>(null);
  const [connectorConfigData, setConnectorConfigData] = createSignal<ConnectorConfigData | null>(
    null,
  );
  const [notification, setNotification] = createSignal<ColonyNotification | null>(null);
  const [connectorOutputs, setConnectorOutputs] = createSignal<ConnectorOutput[]>([]);
  const [availableConnectors, setAvailableConnectors] = createSignal<AvailableConnector[]>([]);
  const [intents, setIntents] = createSignal<Array<{ value: string; label: string }>>([]);
  const [conditionTypes, setConditionTypes] = createSignal<string[]>([]);

  // ── Shared overlays ─────────────────────────────────────
  // W1.A — ModeSelectOverlay is the entry hub before any compose flow.
  const [showModeSelect, setShowModeSelect] = createSignal(false);
  const [showTeamBuilder, setShowTeamBuilder] = createSignal(false);
  const [showRuleBuilder, setShowRuleBuilder] = createSignal(false);
  // F5 — member-picker overlay, scoped to a formation id.
  const [memberPickerFor, setMemberPickerFor] = createSignal<string | null>(null);
  // W1.B — recipe library opens after mode-select. null = closed.
  const [recipeLibraryVariant, setRecipeLibraryVariant] = createSignal<RecipeLibraryVariant | null>(
    null,
  );
  // W1.C — recipe deploy panel; replaces the library until deploy/cancel.
  const [recipeDeploy, setRecipeDeploy] = createSignal<Recipe | null>(null);
  // W1.E — post-deploy proof-of-life panel.
  const [proofOfLife, setProofOfLife] = createSignal<RecipeApplyReport | null>(null);
  // W2.A — seed for TeamBuilder when the user picks CUSTOMIZE on a recipe.
  const [teamBuilderSeed, setTeamBuilderSeed] = createSignal<TeamBuilderSeed | null>(null);
  // W2.B — recipe authoring panel draft.
  const [recipeAuthorDraft, setRecipeAuthorDraft] = createSignal<Recipe | null>(null);

  // ── Pending "pick a connector next" flows ───────────────
  const [pendingAddToFormation, setPendingAddToFormation] = createSignal<string | null>(null);
  const [pendingRecruitToFormation, setPendingRecruitToFormation] = createSignal<string | null>(
    null,
  );
  const [pendingReassignAgent, setPendingReassignAgent] = createSignal<string | null>(null);

  // ── Data → Colony visual model (real data, no fakes) ───
  const nodes = () => mapNodes(db.connectors());
  const agents = () => mapAgents(db.rules(), db.agentStates());
  const formations = () => mapFormations(db.swarms(), db.cooperationEvents());

  // ── Selection → detail-view wiring ─────────────────────
  const selectAgent = (id: string) => {
    setSelection({ id, type: "agent" });
    setDetailView({ mode: "entity" });
  };
  const selectConnector = (id: string) => {
    setSelection({ id, type: "connector" });
    setDetailView({ mode: "entity" });
  };
  const selectFormation = (id: string) => {
    setSelection({ id, type: "formation" });
    setDetailView({ mode: "entity" });
  };
  const clearSelection = () => setSelection({ id: null, type: null });

  const setupConnector = (name: string) => {
    const avail = availableConnectors().find((a) => a.name === name);
    setConnectorConfigData({ id: name, config: {}, configSchema: avail?.config_schema });
  };

  // ── Drag persistence ────────────────────────────────────
  let connectorDragTimer: ReturnType<typeof setTimeout> | undefined;
  const handleConnectorDrag = (id: string, x: number, y: number) => {
    setConnectorPositions((prev) => ({ ...prev, [id]: { x, y } }));
    // Debounced persistence — save to config store after drag settles.
    clearTimeout(connectorDragTimer);
    connectorDragTimer = setTimeout(() => {
      db.provider.setConfig("canvas:connector_positions", connectorPositions()).catch(() => {});
    }, 500);
  };

  // ── Persist locale changes to config store ──────────────
  let localeInitialized = false;
  createEffect(() => {
    const loc = locale();
    if (!localeInitialized) {
      localeInitialized = true;
      return;
    }
    db.provider.setConfig("locale", loc).catch(() => {});
  });

  // ── Persist theme changes to config store ───────────────
  let themeInitialized = false;
  createEffect(() => {
    const t = theme();
    applyTheme(t);
    if (!themeInitialized) {
      themeInitialized = true;
      return;
    }
    db.provider.setConfig("theme", t).catch(() => {});
  });

  /**
   * Load everything the colony view needs, then restore persisted
   * canvas positions, locale and theme. The desktop defers this until
   * the vault is unlocked; the dashboard runs it after settings save.
   */
  const loadColonyData = async () => {
    try {
      await db.refresh();
      // Re-establish the live SSE/Channel streams (canvas, cooperation,
      // events). The initial subscriptions in `createDashboardState` fire at
      // app start — before the vault is unlocked — and fail with "Vault is
      // locked"; this reconnects them now that the runtime is open, so the
      // canvas gets live updates instead of only the one-shot `refresh()`.
      db.resubscribe();
      setAvailableConnectors(await db.provider.listAvailableConnectors());
      setIntents(await db.provider.listIntents());
      const schema = (await db.provider.getRuleSchema()) as Record<string, Record<string, unknown>>;
      if (schema.conditions) {
        setConditionTypes(Object.keys(schema.conditions));
      }
      setConnections((await db.provider.getConnections()) as ColonyConnection[]);

      try {
        const saved = await db.provider.getConfig("canvas:connector_positions");
        if (saved && typeof saved === "object") {
          setConnectorPositions(saved as Record<string, { x: number; y: number }>);
        }
      } catch {
        /* No saved positions — seeded defaults */
      }

      // Restore persisted locale
      try {
        const savedLocale = await db.provider.getConfig("locale");
        if (savedLocale && typeof savedLocale === "string") {
          setLocale(savedLocale as "en");
        }
      } catch {
        /* Default locale is fine */
      }

      // Restore persisted theme
      try {
        const savedTheme = await db.provider.getConfig("theme");
        if (savedTheme && typeof savedTheme === "string") {
          setTheme(savedTheme);
        }
      } catch {
        /* Default theme is fine */
      }
    } catch (e) {
      console.warn("loadColonyData:", e);
    }
  };

  /**
   * True while any overlay owned by this controller is up. The keyboard
   * handler ORs this with the app's own overlay flags so a hotkey never
   * fires behind a modal.
   */
  const overlayOpen = () =>
    showModeSelect() ||
    showTeamBuilder() ||
    showRuleBuilder() ||
    memberPickerFor() !== null ||
    recipeLibraryVariant() !== null ||
    recipeDeploy() !== null ||
    proofOfLife() !== null ||
    recipeAuthorDraft() !== null ||
    confirmAction() !== null ||
    aiConfigAgent() !== null ||
    connectorConfigData() !== null ||
    (opts.appOverlayOpen?.() ?? false);

  // ── Command dispatch — context:action pattern ───────────
  // Each command has a unique action ID (e.g. "connector:enable",
  // "agent:pause") so there's no ambiguity between contexts. Every case
  // maps to a real backend operation or opens a real UI panel.
  const handleCommand = async (action: string) => {
    const sel = selection();
    try {
      switch (action) {
        // ── Global ──
        case "global:new_rule":
          // G5e — open the visual rule builder. TeamBuilder is the
          // formation-spawning wizard, available via a different entry
          // point; this command targets rule composition specifically.
          setShowRuleBuilder(true);
          break;
        case "global:make_bot":
          // Canvas is live so a refresh command is redundant — this slot
          // routes the user back to the bot/team selection hub so they can
          // add another bot once the canvas already has some.
          clearSelection();
          setShowModeSelect(true);
          break;
        case "global:connectors":
          clearSelection();
          setDetailView({ mode: "connectors" });
          setAvailableConnectors(await db.provider.listAvailableConnectors());
          await db.refresh();
          break;
        case "global:events":
          clearSelection();
          setDetailView({ mode: "events" });
          break;
        case "global:bots":
          clearSelection();
          setDetailView({ mode: "bots" });
          break;
        case "global:settings":
          opts.onOpenSettings();
          break;
        case "global:chat":
          // W5 — open the in-app chat surface (the "ASK" primary action).
          opts.onOpenChat();
          break;

        // ── Tree (connector selected) ──
        case "connector:enable":
          if (sel.id) {
            await db.provider.enableConnector(sel.id);
            await db.refresh();
          }
          break;
        case "connector:disable":
          if (sel.id) {
            await db.provider.disableConnector(sel.id);
            await db.refresh();
          }
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
            const targetId = sel.id;
            const deps = await db.provider.listRulesForConnector(targetId);
            setConfirmAction({
              title: "Remove Connector",
              message: `Remove ${targetId} and ${deps.length} dependent rule(s)?`,
              label: "Remove",
              action: async () => {
                await db.provider.removeConnectorCascade(targetId);
                clearSelection();
                await db.refresh();
              },
            });
          }
          break;
        case "connector:events":
          if (sel.id) {
            setDetailView({ mode: "events", filterConnector: sel.id });
          }
          break;
        case "connector:test":
          if (sel.id) {
            try {
              const result = await db.provider.testConnector(sel.id);
              setNotification({
                message: result.matched
                  ? `Test passed: "${result.rule_name}"`
                  : `No match: "${result.rule_name}"`,
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
        // Plan 3.3: PAUSE disables, RESUME enables. Both used to disable
        // (RECALL was a duplicate PAUSE), so a paused bot could not be
        // restarted from the command card at all.
        case "agent:pause":
          if (sel.id) {
            await db.provider.toggleRule(sel.id, false);
            await db.refresh();
          }
          break;
        case "agent:resume":
          if (sel.id) {
            await db.provider.toggleRule(sel.id, true);
            await db.refresh();
          }
          break;
        case "agent:detach":
          if (sel.id) {
            const targetId = sel.id;
            setConfirmAction({
              title: "Detach Agent",
              message: "This will delete the rule. The agent will be removed from the colony.",
              label: "Detach",
              action: async () => {
                await db.handleDelete(targetId);
                clearSelection();
                await db.refresh();
              },
            });
          }
          break;
        case "agent:inspect":
          // Plan 3.3: INSPECT opens the run history (ExecutionsPanel), not
          // the entity view the user is already looking at.
          if (sel.id) {
            setDetailView({ mode: "reports", ruleId: sel.id });
          }
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
          if (sel.id) {
            await db.provider.stepAutonomy(sel.id, "up");
            await db.refresh();
          }
          break;
        case "agent:autonomy_down":
          if (sel.id) {
            await db.provider.stepAutonomy(sel.id, "down");
            await db.refresh();
          }
          break;

        // ── Formation (swarm selected) ──
        // Parameterless lifecycle/capability commands → backend generic
        // dispatcher (`run_formation_command`). The frontend forwards the
        // clicked id; ALL command→action mapping lives in Rust.
        case "formation:deploy":
        case "formation:pause":
        case "formation:resume":
        case "formation:rally":
        case "formation:intent":
        case "formation:guard":
          if (sel.id) {
            await db.provider.runFormationCommand(sel.id, action);
            await db.refresh();
          }
          break;
        case "formation:dissolve":
          if (sel.id) {
            const targetId = sel.id;
            setConfirmAction({
              title: "Dissolve Formation",
              message: "All agents will be released from this formation.",
              label: "Dissolve",
              action: async () => {
                await db.provider.runFormationCommand(targetId, "formation:dissolve");
                clearSelection();
                await db.refresh();
              },
            });
          }
          break;
        case "formation:add_member":
          // Plan 3.3: the backend emits `formation:add_member` (the old
          // `formation:add` case was never reachable). Pick the connector,
          // then dispatch through `runFormationCommand` with its param.
          if (sel.id) {
            setPendingAddToFormation(sel.id);
            setDetailView({ mode: "connectors" });
          }
          break;
        case "formation:remove_member":
          // F5 — open the member-picker overlay scoped to this formation.
          // The overlay fetches the eligible-removal list (B11
          // `formation_eligible_members`) and renders one button per member.
          if (sel.id) {
            setMemberPickerFor(sel.id);
          }
          break;
        case "formation:recruit":
          // §7 Fever recruit — pick a connector; the backend gates on
          // momentum tier (Fever) + guard mode before adding the member.
          if (sel.id) {
            setPendingRecruitToFormation(sel.id);
            setDetailView({ mode: "connectors" });
          }
          break;
        case "formation:ai_adapter":
          // G7 — open the per-formation AI override panel (dispatched by
          // `FormationAiAdapterRow`, not by the command card). Uses the
          // shared AiConfigPanel scoped to a formation; on save the config
          // persists at `ai:formation:{id}` and the next Fever-tier
          // orchestrate call picks it up via `resolve_ai_config`.
          if (sel.id) {
            const fm = db.swarms().find((s) => s.id === sel.id);
            setAiConfigAgent({ id: sel.id, name: fm?.name ?? sel.id, scope: "formation" });
          }
          break;
      }
    } catch (e) {
      db.setError(String(e));
    }
  };

  /**
   * Bottom-panel connector click. Resolves whichever "pick a connector
   * next" flow is pending (reassign / recruit / add-to-formation) before
   * falling back to plain selection.
   */
  const handleSelectConnectorFromPanel = async (id: string) => {
    const reassignId = pendingReassignAgent();
    const recruitId = pendingRecruitToFormation();
    const formationId = pendingAddToFormation();
    if (reassignId) {
      await db.provider.reassignRuleConnector(reassignId, id);
      setPendingReassignAgent(null);
      setDetailView({ mode: "entity" });
      await db.refresh();
    } else if (recruitId) {
      // §7 Fever recruit — backend gates on momentum tier + guard.
      await db.provider.runFormationCommand(recruitId, "formation:recruit", {
        connector_name: id,
      });
      setPendingRecruitToFormation(null);
      setDetailView({ mode: "entity" });
      await db.refresh();
    } else if (formationId) {
      await db.provider.runFormationCommand(formationId, "formation:add_member", {
        connector_name: id,
      });
      setPendingAddToFormation(null);
      setDetailView({ mode: "entity" });
      await db.refresh();
    } else {
      selectConnector(id);
    }
  };

  /** Bottom-panel "add to formation" shortcut. */
  const handleAddToFormation = async (formationId: string, connectorName: string) => {
    await db.provider.runFormationCommand(formationId, "formation:add_member", {
      connector_name: connectorName,
    });
    setDetailView({ mode: "entity" });
    await db.refresh();
  };

  // W1.A — Mode-select hub handler. Each card opens its own flow. Per
  // `feedback_multi_path_oobe`, the hub doesn't carry state across modes —
  // picking a mode dismisses the hub and opens that mode's entry surface.
  const handleModeSelect = (mode: CreateMode) => {
    setShowModeSelect(false);
    switch (mode) {
      case "bot":
        setRecipeLibraryVariant("bot");
        break;
      case "team":
        // W1.B's library covers single-bot recipes today; team recipes
        // arrive in a later wave. Open the team-aware variant so the user
        // sees what's there and can fall through to TeamBuilder.
        setRecipeLibraryVariant("team");
        break;
      case "addToTeam":
        // Until we ship the "pick a team" picker, open the library so the
        // user can clone a recipe into a new agent.
        setRecipeLibraryVariant("all");
        break;
    }
  };

  const handleUseRecipe = (recipe: Recipe) => {
    setRecipeLibraryVariant(null);
    setRecipeDeploy(recipe);
  };

  /** Close every overlay this controller owns (Escape, and app callers). */
  const closeSharedOverlays = () => {
    clearSelection();
    setConfirmAction(null);
    setShowTeamBuilder(false);
    setShowModeSelect(false);
    setAiConfigAgent(null);
    setConnectorConfigData(null);
  };

  // ── Keyboard shortcuts ─────────────────────────────────
  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement
      )
        return;

      // App-owned bindings first (desktop's Ctrl+Shift+Q quick-exit).
      if (opts.onKeyDown?.(e)) return;

      const key = e.key.toLowerCase();

      // 1-9: select agent by index
      if (key >= "1" && key <= "9") {
        const idx = parseInt(key, 10) - 1;
        const agent = agents()[idx];
        if (agent) {
          setSelection({ id: agent.id, type: "agent" });
          // Plan 3.7: number keys select AND move focus to the sprite.
          document
            .querySelector<HTMLElement>(
              `button.colony-agent[data-agent-id="${CSS.escape(agent.id)}"]`,
            )
            ?.focus();
        }
        return;
      }

      // Escape: clear selection and close overlays
      if (key === "escape") {
        closeSharedOverlays();
        opts.onEscape?.();
        return;
      }

      const modalOpen = overlayOpen();

      // Plan 3.6 — `O` cycles the canvas overlay (none → momentum →
      // attention → fuel), the ONI convention. W2.E's canvas (OUTPUT) view
      // moves to Shift+O so both keep the same mnemonic without colliding.
      if (key === "o" && !e.ctrlKey && !e.metaKey && !modalOpen) {
        e.preventDefault();
        if (e.shiftKey) setDetailView({ mode: "canvas" });
        else setOverlay(nextOverlay(overlay()));
        return;
      }

      // W1.A — `N` opens the mode-select hub (the entry to every compose
      // flow). Skipped when any other modal is open.
      if (key === "n" && !e.ctrlKey && !e.metaKey && !e.shiftKey && !modalOpen) {
        e.preventDefault();
        setShowModeSelect(true);
        return;
      }

      // Skip command shortcuts when any modal is open
      if (modalOpen) return;

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

  return {
    // Visual model
    nodes,
    agents,
    formations,
    connections,
    setConnections,
    // Selection + detail view
    selection,
    setSelection,
    overlay,
    setOverlay,
    detailView,
    setDetailView,
    selectAgent,
    selectConnector,
    selectFormation,
    clearSelection,
    // Canvas
    connectorPositions,
    setConnectorPositions,
    handleConnectorDrag,
    // Theme
    theme,
    setTheme,
    // Reference data
    availableConnectors,
    setAvailableConnectors,
    intents,
    conditionTypes,
    connectorOutputs,
    setupConnector,
    // Shared overlays
    confirmAction,
    setConfirmAction,
    aiConfigAgent,
    setAiConfigAgent,
    connectorConfigData,
    setConnectorConfigData,
    notification,
    showModeSelect,
    setShowModeSelect,
    showTeamBuilder,
    setShowTeamBuilder,
    showRuleBuilder,
    setShowRuleBuilder,
    memberPickerFor,
    setMemberPickerFor,
    recipeLibraryVariant,
    setRecipeLibraryVariant,
    recipeDeploy,
    setRecipeDeploy,
    proofOfLife,
    setProofOfLife,
    teamBuilderSeed,
    setTeamBuilderSeed,
    recipeAuthorDraft,
    setRecipeAuthorDraft,
    overlayOpen,
    closeSharedOverlays,
    // Handlers
    handleCommand,
    handleModeSelect,
    handleUseRecipe,
    handleSelectConnectorFromPanel,
    handleAddToFormation,
    loadColonyData,
  };
}
