/**
 * Desktop data provider — wraps Tauri IPC modules into DataProvider interface.
 *
 * Thin adapter: no logic, just maps existing ipc/ functions to the
 * platform-agnostic DataProvider that createDashboardState() expects.
 */

import type { CanvasUpdate, EventEntry } from "@springtale/types";
import type { CooperationEventEnvelope, DataProvider, UtteranceDefs } from "@springtale/ui";
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getAutonomy, listAgentStates, setAutonomy, stepAutonomy } from "./ipc/agents";
import { listPendingApprovals, resolveApproval as resolveRuntimeApproval } from "./ipc/approval";
import { addAuthor, listAuthors, removeAuthor } from "./ipc/authors";
import { getCanvasState, getConnections } from "./ipc/canvas";
import {
  auditMemory,
  compactMemory,
  configureAiAdapter,
  exportData,
  getConfig,
  listConfig,
  setAiAdapter,
  setConfig,
  setConnectorConfig,
  toggleFormationGuard,
  upsertConnectorConfig,
} from "./ipc/config";
import {
  disableConnector,
  enableConnector,
  getConnectorConfig,
  getConnectorSchemas,
  listAvailableConnectors,
  listConnectorOutputs,
  listConnectors,
  reloadConnector,
  removeConnector,
  removeConnectorCascade,
  setupConnector,
} from "./ipc/connectors";
import { runDiagnostics } from "./ipc/diagnostics";
import { getRecipeDrift } from "./ipc/drift";
import { listEvents } from "./ipc/events";
import { getExecutionSteps, listExecutions } from "./ipc/executions";
import { applyFix, getFix, listFixes } from "./ipc/fixes";
import {
  addFormationMember,
  createFormation,
  cycleFormationAutonomy,
  cycleFormationIntent,
  deployFormation,
  deployTeam,
  dissolveFormation,
  formationAvailableCommands,
  formationEligibleMembers,
  getFormation,
  listFormations,
  listIntents,
  pauseFormation,
  rallyFormation,
  removeFormationMember,
  resumeFormation,
  runFormationCommand,
  updateFormationIntent,
} from "./ipc/formations";
import { applyOnboarding, listOnboardingPlatforms } from "./ipc/onboarding";
import {
  applyRecipe,
  deleteUserRecipe,
  exportRecipeToml,
  forkRecipe,
  getRecipe,
  importRecipeToml,
  listRecipeCategories,
  listRecipePieces,
  listRecipes,
  preflightRecipe,
  previewRecipe,
  recordRecipeRecent,
  renderRecipeToml,
  saveUserRecipe,
  toggleRecipeFavorite,
} from "./ipc/recipes";
import {
  createConnectorRule,
  createRule,
  deleteRule,
  getRuleSchema,
  listRules,
  listRulesForConnector,
  parseRuleFromIntent,
  reassignRuleConnector,
  runRule,
  testConnector,
  toggleRule,
  updateRule,
} from "./ipc/rules";
import {
  getSafetyConfig as ipcGetSafetyConfig,
  saveSafetyConfig as ipcSaveSafetyConfig,
  setDisguiseActive as ipcSetDisguiseActive,
  setDisguiseProfile as ipcSetDisguiseProfile,
  setPanicTapCount as ipcSetPanicTapCount,
} from "./ipc/safety";
import { openSelectorPicker } from "./ipc/selector_picker";
import { sendMessage } from "./ipc/send";
import { listTemplates, writeTemplate } from "./ipc/templates";
import { testRecipeStep } from "./ipc/test_step";
import {
  cancelOnboardStream,
  deleteWorkspace,
  listWorkspaces,
  previewOnboardUrl,
  scanWorkspaces,
  startOnboardStream,
  subscribeToChatDiscovered,
  upsertWorkspaceManual,
} from "./ipc/workspaces";

export function createDesktopProvider(): DataProvider {
  return {
    // Connectors
    listConnectors,
    listAvailableConnectors,
    setupConnector: (name: string, config: Record<string, unknown>) => setupConnector(name, config),
    getConnectorSchemas,
    enableConnector: (name: string) => enableConnector(name),
    disableConnector: (name: string) => disableConnector(name),
    reloadConnector: (name: string) => reloadConnector(name),
    removeConnector: (name: string) => removeConnector(name),
    removeConnectorCascade: (name: string) => removeConnectorCascade(name),
    getConnectorConfig: (name: string) => getConnectorConfig(name),
    listConnectorOutputs: (name: string, limit?: number) => listConnectorOutputs(name, limit),

    // Rules
    listRules,
    createRule,
    toggleRule,
    deleteRule,
    updateRule,
    runRule,
    parseRuleFromIntent,
    createConnectorRule,
    getRuleSchema,
    listRulesForConnector,
    testConnector,
    reassignRuleConnector,

    // Phase B — Executions log
    listExecutions,
    getExecutionSteps,

    // Phase B — Selector picker (authoring-time)
    openSelectorPicker,

    // Phase C — Test This Step + drift detection
    testRecipeStep,
    getRecipeDrift,

    // D1 — External-workspace directory
    listWorkspaces,
    scanWorkspaces,
    deleteWorkspace,
    upsertWorkspaceManual,
    previewOnboardUrl,
    startOnboardStream,
    cancelOnboardStream,
    subscribeToChatDiscovered,

    // Events
    listEvents,
    // Plan 6.7 — the runtime chat gate's queue (`list_pending_approvals` /
    // `resolve_approval`). Sentinel prompts are a separate queue: they
    // arrive as `approval-required` Tauri events (App.tsx overlay) and are
    // answered via `respond_to_approval`.
    listApprovals: listPendingApprovals,
    resolveApproval: resolveRuntimeApproval,
    subscribeToEvents(callback) {
      let unlisten: (() => void) | undefined;
      listen<EventEntry>("event-fired", (e) => callback(e.payload))
        .then((u) => {
          unlisten = u;
        })
        .catch(() => {});
      return () => unlisten?.();
    },

    // In-app chat — the desktop runs the same bot loop the daemon does
    // (built in state::init_runtime). `send_chat_message` injects the turn;
    // replies arrive as `chat-message` Tauri events.
    async sendChatMessage(text, session) {
      await invoke("send_chat_message", { text, session: session ?? null });
    },
    subscribeToChat(callback) {
      let unlisten: (() => void) | undefined;
      listen<{ session: string; text: string }>("chat-message", (e) => callback(e.payload))
        .then((u) => {
          unlisten = u;
        })
        .catch(() => {});
      return () => unlisten?.();
    },

    // Formations
    getFormation,
    listFormations,
    createFormation,
    deployFormation,
    pauseFormation,
    resumeFormation,
    dissolveFormation,
    rallyFormation,
    updateFormationIntent,
    addFormationMember,
    removeFormationMember,
    listIntents,
    deployTeam,
    cycleFormationIntent,
    cycleFormationAutonomy,
    formationAvailableCommands,
    runFormationCommand,
    formationEligibleMembers,

    // Config
    getConfig,
    setConfig,
    listConfig,
    setAiAdapter,
    setConnectorConfig,
    configureAiAdapter,
    upsertConnectorConfig,
    toggleFormationGuard,

    // Agent state + autonomy
    listAgentStates,
    getAutonomy,
    setAutonomy,
    stepAutonomy,

    // Trusted authors
    listAuthors,
    addAuthor,
    removeAuthor,

    // Data
    exportData,

    // Memory
    auditMemory,
    compactMemory,

    // Canvas
    getConnections,
    getCanvasState,
    async getUtteranceDefs() {
      return invoke<UtteranceDefs>("utterance_defs");
    },
    subscribeToCanvasUpdates(callback) {
      // F4 + E10: Tauri 2 `Channel<T>` is the right primitive for streaming
      // — purpose-built for high-rate events vs broadcast `emit()`. The
      // backend `subscribe_canvas` command (see
      // `tauri/apps/desktop/src-tauri/src/commands/canvas.rs`) spawns a
      // forwarder that reads from `runtime.canvas_tx` and writes to this
      // channel, mirroring the web dashboard's `/canvas/stream` SSE path.
      const channel = new Channel<CanvasUpdate>();
      channel.onmessage = (update) => callback(update);
      invoke("subscribe_canvas", { channel }).catch((e) => {
        // Expected before vault unlock — the runtime isn't open yet. The
        // post-unlock `resubscribe()` re-establishes the stream, so this is
        // not an error worth surfacing. Real failures still log.
        if (!String(e).includes("Vault is locked")) console.warn("subscribe_canvas failed:", e);
      });
      return () => {
        // The forwarder breaks its loop when send returns Err on channel
        // drop; nothing else to clean up frontend-side.
      };
    },

    subscribeToCooperationEvents(callback) {
      // Phase H4 — verbatim mirror of subscribeToCanvasUpdates above.
      // Backend `subscribe_cooperation` command spawns a forwarder from
      // `runtime.cooperation_tx` into this Channel; mirrors the web
      // dashboard's `/cooperation/events` SSE endpoint.
      const channel = new Channel<CooperationEventEnvelope>();
      channel.onmessage = (envelope) => callback(envelope);
      invoke("subscribe_cooperation", { channel }).catch((e) => {
        // Expected before unlock; `resubscribe()` re-establishes it after.
        if (!String(e).includes("Vault is locked"))
          console.warn("subscribe_cooperation failed:", e);
      });
      return () => {
        // Forwarder loop ends on channel drop.
      };
    },

    // G5d — IPV duress surface (desktop).
    getSafetyConfig: () => ipcGetSafetyConfig(),
    saveSafetyConfig: (config) => ipcSaveSafetyConfig(config),
    setDisguiseActive: (active: boolean) => ipcSetDisguiseActive(active),
    setDisguiseProfile: (appName: string, iconId: string) => ipcSetDisguiseProfile(appName, iconId),
    setPanicTapCount: (count: number) => ipcSetPanicTapCount(count),

    // Diagnostics
    runDiagnostics,

    // Onboarding
    listOnboardingPlatforms,
    applyOnboarding,
    // W1.B recipes
    listRecipes,
    getRecipe,
    listRecipeCategories,
    toggleRecipeFavorite,
    recordRecipeRecent,
    // W1.C deploy
    applyRecipe,
    renderRecipeToml,
    // W1.D preflight
    preflightRecipe,
    // W2.C preview
    previewRecipe,
    // W2.D modular pieces
    listRecipePieces,
    // W2.B authoring
    saveUserRecipe,
    forkRecipe,
    deleteUserRecipe,
    exportRecipeToml,
    importRecipeToml,

    // Templates
    listTemplates,
    writeTemplate,

    // Error fixes
    listFixes,
    getFix,
    applyFix,

    // Cross-channel send
    sendMessage,
  };
}
