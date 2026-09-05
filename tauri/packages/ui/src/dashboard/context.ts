/**
 * Shared dashboard state — createDashboardState() factory + context.
 *
 * Follows the exact pattern of createI18n() in i18n/context.ts:
 * factory function → context → provider → hook.
 *
 * Both the Tauri desktop app and the web dashboard call this with
 * their platform-specific DataProvider. The returned DashboardState
 * drives the RTS layout (ResourceBar + Roster + Canvas + CommandPanel).
 */

import type {
  AgentState,
  CanvasState,
  CanvasUpdate,
  ConnectorSchema,
  EventEntry,
} from "@springtale/types";
import {
  createContext,
  createResource,
  createRoot,
  createSignal,
  onCleanup,
  useContext,
} from "solid-js";
import type { ConditionDef } from "../ConditionEditor";
import type {
  ConnectorStatus,
  EventItem,
  RuleDetail,
  RuleItem,
  SwarmInfo,
} from "../dashboard/model";
import type { ApprovalInfo, CooperationEventEnvelope, DashboardState, DataProvider } from "./types";

// ── Canvas update reducer ────────────────────────────────
// Extracted from apps/dashboard/src/pages/Canvas.tsx

function applyCanvasUpdate(current: CanvasState | null, update: CanvasUpdate): CanvasState {
  const base: CanvasState = current ?? { blocks: [], updated_at: new Date().toISOString() };
  const blocks = [...base.blocks];

  switch (update.action) {
    case "SetBlocks":
      return { ...base, blocks: update.blocks, updated_at: new Date().toISOString() };
    case "UpdateBlock": {
      const idx = blocks.findIndex((b) => b.id === update.id);
      if (idx >= 0) {
        blocks[idx] = update.block;
      } else {
        blocks.push(update.block);
      }
      return { ...base, blocks, updated_at: new Date().toISOString() };
    }
    case "RemoveBlock":
      return {
        ...base,
        blocks: blocks.filter((b) => b.id !== update.id),
        updated_at: new Date().toISOString(),
      };
    case "Clear":
      return { ...base, blocks: [], updated_at: new Date().toISOString() };
    default:
      return base;
  }
}

// ── Factory ──────────────────────────────────────────────

export function createDashboardState(provider: DataProvider): DashboardState {
  // Wrap the entire factory body in `createRoot` so every reactive
  // primitive below (signals, resource, subscription onCleanups) has
  // a real reactive owner. This factory is called at module-load
  // time from `main.tsx`, BEFORE `render()` creates its own root —
  // without this wrapper every primitive is an orphan computation
  // (the source of "computations created outside a createRoot"
  // warnings) and SolidJS's lifecycle bookkeeping loses track of
  // them, which surfaces as `_el$N.nextSibling = null` template-
  // walker crashes when downstream components mount Suspense +
  // Switch pipelines on top of the corrupted owner graph. The root
  // intentionally never disposes — the dashboard state lives for
  // the entire app session.
  //
  // Per https://docs.solidjs.com/reference/reactive-utilities/create-root:
  // "Creates a non-tracked owner detached from the parent."
  return createRoot(() => {
    // ── Core data signals ──
    const [connectors, setConnectors] = createSignal<ConnectorStatus[]>([]);
    const [schemas, setSchemas] = createSignal<ConnectorSchema[]>([]);
    const [rules, setRules] = createSignal<RuleItem[]>([]);
    const [events, setEvents] = createSignal<EventItem[]>([]);
    const [swarms, setSwarms] = createSignal<SwarmInfo[]>([]);
    const [agentStates, setAgentStates] = createSignal<AgentState[]>([]);
    const [canvasState, setCanvasState] = createSignal<CanvasState | null>(null);
    // Phase H — cooperation events ring (last 200 envelopes). Drives the
    // EventRibbon toast + BottomPanel formation event log.
    const [cooperationEvents, setCooperationEvents] = createSignal<CooperationEventEnvelope[]>([]);

    // Plan 6.7 — pending approvals (chat gate queue). Reloaded when an
    // `approval_required` event lands on the stream and after each resolve.
    const [pendingApprovals, setPendingApprovals] = createSignal<ApprovalInfo[]>([]);
    const refreshApprovals = async () => {
      try {
        setPendingApprovals(await provider.listApprovals());
      } catch {
        // Gate not wired or offline — keep the last known queue.
      }
    };
    const resolveApproval = async (id: string, approve: boolean) => {
      await provider.resolveApproval(id, approve);
      await refreshApprovals();
    };
    void refreshApprovals();
    const [error, setError] = createSignal("");
    const [loading, setLoading] = createSignal(true);

    // ── Selection signals ──
    const [selectedRuleId, setSelectedRuleId] = createSignal<string | null>(null);
    const [selectedSwarmId, setSelectedSwarmId] = createSignal<string | null>(null);

    // ── UI panel signals ──
    const [showNewRule, setShowNewRule] = createSignal(false);

    // ── Rule builder form signals ──
    const [newRuleName, setNewRuleName] = createSignal("");
    const [triggerConnector, setTriggerConnector] = createSignal("");
    const [triggerName, setTriggerName] = createSignal("");
    const [actionConnector, setActionConnector] = createSignal("");
    const [actionName, setActionName] = createSignal("");
    const [conditions, setConditions] = createSignal<ConditionDef[]>([]);

    // ── SSE subscriptions ──
    let unsubEvents = provider.subscribeToEvents((event: EventEntry) => {
      if (event.trigger_type === "approval_required") void refreshApprovals();
      setEvents((prev: EventItem[]) =>
        [
          {
            id: event.id,
            connectorName: event.connector_name,
            triggerType: event.trigger_type,
            timestamp: event.timestamp,
            actionTaken: event.action_taken,
            severity:
              (event as EventEntry & { severity?: string }).severity === "error"
                ? ("error" as const)
                : ("ok" as const),
          },
          ...prev,
        ].slice(0, 200),
      );
    });

    let unsubCanvas = provider.subscribeToCanvasUpdates((update: CanvasUpdate) => {
      setCanvasState((prev: CanvasState | null) => applyCanvasUpdate(prev, update));
    });

    // Phase H — cooperation events SSE / Tauri Channel subscription. Keep
    // the last 200 envelopes so the EventRibbon + BottomPanel log have
    // enough history without unbounded growth.
    let unsubCooperation = provider.subscribeToCooperationEvents(
      (envelope: CooperationEventEnvelope) => {
        setCooperationEvents((prev) => [envelope, ...prev].slice(0, 200));
      },
    );

    const resubscribe = () => {
      unsubEvents();
      unsubCanvas();
      unsubCooperation();
      unsubEvents = provider.subscribeToEvents((event: EventEntry) => {
        if (event.trigger_type === "approval_required") void refreshApprovals();
        setEvents((prev: EventItem[]) =>
          [
            {
              id: event.id,
              connectorName: event.connector_name,
              triggerType: event.trigger_type,
              timestamp: event.timestamp,
              actionTaken: event.action_taken,
              severity:
                (event as EventEntry & { severity?: string }).severity === "error"
                  ? ("error" as const)
                  : ("ok" as const),
            },
            ...prev,
          ].slice(0, 200),
        );
      });
      unsubCanvas = provider.subscribeToCanvasUpdates((update: CanvasUpdate) => {
        setCanvasState((prev: CanvasState | null) => applyCanvasUpdate(prev, update));
      });
      unsubCooperation = provider.subscribeToCooperationEvents(
        (envelope: CooperationEventEnvelope) => {
          setCooperationEvents((prev) => [envelope, ...prev].slice(0, 200));
        },
      );
    };

    onCleanup(() => {
      unsubEvents();
      unsubCanvas();
      unsubCooperation();
    });

    // ── Refresh (bulk data load) ──

    const refresh = async () => {
      setLoading(true);
      try {
        const [c, r, e, s, cs, as_] = await Promise.all([
          provider.listConnectors(),
          provider.listRules(),
          provider.listEvents(20),
          provider.listFormations(),
          provider.getConnectorSchemas(),
          provider.listAgentStates(),
        ]);

        setConnectors(c.map((x) => ({ name: x.name, enabled: x.enabled })));

        setRules(
          r.map((x) => ({
            id: x.id,
            name: x.name,
            status: x.status,
            triggerType: x.trigger_type,
            connector: x.connector_name ?? x.trigger_type,
          })),
        );

        setEvents(
          e.map((x) => {
            const severity = (x as EventEntry & { severity?: string }).severity;
            return {
              id: x.id,
              connectorName: x.connector_name,
              triggerType: x.trigger_type,
              timestamp: x.timestamp,
              actionTaken: x.action_taken,
              severity: severity === "error" ? ("error" as const) : ("ok" as const),
            };
          }),
        );

        setSwarms(
          s.map((x) => ({
            id: x.id,
            name: x.name,
            intent: x.intent,
            status: x.status,
            member_count: x.member_count,
            members: x.members ?? [],
            momentum_tier: x.momentum_tier,
            momentum_label: x.momentum_label,
            capabilities: x.capabilities,
            guard_status: x.guard_status,
          })),
        );

        setSchemas(cs);
        setAgentStates(as_);
      } catch {
        // First launch — store may be empty
      }

      try {
        const canvas = await provider.getCanvasState();
        setCanvasState(canvas);
      } catch {
        // Canvas not initialized yet
      }

      setLoading(false);
    };

    // ── Action handlers ──

    const handleToggle = async (id: string, currentlyEnabled: boolean) => {
      try {
        await provider.toggleRule(id, !currentlyEnabled);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleDelete = async (id: string) => {
      try {
        await provider.deleteRule(id);
        setSelectedRuleId(null);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleSaveNewRule = async () => {
      if (!newRuleName() || !triggerName() || !actionName()) return;
      try {
        await provider.createConnectorRule({
          name: newRuleName(),
          trigger_connector: triggerConnector(),
          trigger_event: triggerName(),
          action_connector: actionConnector(),
          action_name: actionName(),
          conditions: conditions(),
        });
        setShowNewRule(false);
        setNewRuleName("");
        setTriggerConnector("");
        setTriggerName("");
        setActionConnector("");
        setActionName("");
        setConditions([]);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleHatch = async (
      name: string,
      intent: string,
      _hatchConnectors: string[],
      trigC: string,
      trigE: string,
      actC: string,
      actN: string,
    ) => {
      try {
        await provider.deployTeam({
          name,
          intent,
          guard_mode: false,
          agents: [
            {
              connector_name: trigC,
              trigger_name: trigE,
              action_connector: actC,
              action_name: actN,
            },
          ],
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleDeployFormation = async (id: string) => {
      try {
        await provider.deployFormation(id);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handlePauseFormation = async (id: string) => {
      try {
        await provider.pauseFormation(id);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleResumeFormation = async (id: string) => {
      try {
        await provider.resumeFormation(id);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleDissolveFormation = async (id: string) => {
      try {
        await provider.dissolveFormation(id);
        setSelectedSwarmId(null);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    const handleRallyFormation = async (id: string) => {
      try {
        await provider.rallyFormation(id);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    };

    // ── Derived ──

    const selectedRule = (): RuleDetail | null => {
      const id = selectedRuleId();
      if (!id) return null;
      const r = rules().find((x) => x.id === id);
      if (!r) return null;
      return {
        id: r.id,
        name: r.name,
        status: r.status,
        triggerType: r.triggerType,
        triggerConfig: r.connector ?? r.triggerType,
        conditions: [],
        actions: [],
      };
    };

    // F1 + B11: backend-supplied formation command list. Resource re-fetches
    // when the selected swarm changes; status-aware enable/disable + canonical
    // hotkey live in Rust per the thin-frontend rule.
    const [formationCommandsResource] = createResource(
      () => selectedSwarmId(),
      async (id) => {
        if (!id) return undefined;
        try {
          return await provider.formationAvailableCommands(id);
        } catch (e) {
          setError(String(e));
          return undefined;
        }
      },
    );

    return {
      // Core data
      connectors,
      schemas,
      rules,
      events,
      swarms,
      agentStates,
      canvasState,
      cooperationEvents,
      pendingApprovals,
      refreshApprovals,
      resolveApproval,
      error,
      loading,
      formationCommands: () => formationCommandsResource(),
      // Selection
      selectedRuleId,
      setSelectedRuleId,
      selectedSwarmId,
      setSelectedSwarmId,
      // UI panels
      showNewRule,
      setShowNewRule,
      // Rule builder form
      newRuleName,
      setNewRuleName,
      triggerConnector,
      setTriggerConnector,
      triggerName,
      setTriggerName,
      actionConnector,
      setActionConnector,
      actionName,
      setActionName,
      conditions,
      setConditions,
      // Error
      setError,
      clearError: () => setError(""),
      // Actions
      refresh,
      handleToggle,
      handleDelete,
      handleSaveNewRule,
      handleHatch,
      handleDeployFormation,
      handlePauseFormation,
      handleResumeFormation,
      handleDissolveFormation,
      handleRallyFormation,
      // Derived
      selectedRule,
      // Provider + resubscribe
      provider,
      resubscribe,
    };
  }); // close createRoot
}

// ── Context wiring ───────────────────────────────────────

const DashboardContext = createContext<DashboardState>();

export const DashboardProvider = DashboardContext.Provider;

export function useDashboard(): DashboardState {
  const ctx = useContext(DashboardContext);
  if (!ctx) {
    throw new Error("useDashboard must be used within DashboardProvider");
  }
  return ctx;
}
