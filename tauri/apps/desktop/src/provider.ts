/**
 * Desktop data provider — wraps Tauri IPC modules into DataProvider interface.
 *
 * Thin adapter: no logic, just maps existing ipc/ functions to the
 * platform-agnostic DataProvider that createDashboardState() expects.
 */
import type { DataProvider, CooperationEventEnvelope } from "@springtale/ui";
import { listen } from "@tauri-apps/api/event";
import { Channel, invoke } from "@tauri-apps/api/core";
import type { EventEntry, CanvasUpdate } from "@springtale/types";

import { listConnectors, listAvailableConnectors, setupConnector, getConnectorSchemas, enableConnector, disableConnector, reloadConnector, removeConnector, removeConnectorCascade, getConnectorConfig, listConnectorOutputs } from "./ipc/connectors";
import { listRules, createRule, toggleRule, deleteRule, updateRule, runRule, parseRuleFromIntent, listRulesForConnector, testConnector, reassignRuleConnector, createConnectorRule, getRuleSchema } from "./ipc/rules";
import { listEvents } from "./ipc/events";
import { listExecutions, getExecutionSteps } from "./ipc/executions";
import { openSelectorPicker } from "./ipc/selector_picker";
import { testRecipeStep } from "./ipc/test_step";
import { getRecipeDrift } from "./ipc/drift";
import {
  listWorkspaces,
  scanWorkspaces,
  deleteWorkspace,
  upsertWorkspaceManual,
  previewOnboardUrl,
  startOnboardStream,
  cancelOnboardStream,
  subscribeToChatDiscovered,
} from "./ipc/workspaces";
import {
  listFormations,
  getFormation,
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
  formationEligibleMembers,
} from "./ipc/formations";
import { getCanvasState, getConnections } from "./ipc/canvas";
import {
  getConfig, setConfig, listConfig, setAiAdapter, setConnectorConfig,
  configureAiAdapter, upsertConnectorConfig, toggleFormationGuard,
  exportData, auditMemory, compactMemory,
} from "./ipc/config";
import { listAgentStates, getAutonomy, setAutonomy, stepAutonomy } from "./ipc/agents";
import { listAuthors, addAuthor, removeAuthor } from "./ipc/authors";
import { runDiagnostics } from "./ipc/diagnostics";
import {
  setDisguiseActive as ipcSetDisguiseActive,
  setDisguiseProfile as ipcSetDisguiseProfile,
  setPanicTapCount as ipcSetPanicTapCount,
  getSafetyConfig as ipcGetSafetyConfig,
  saveSafetyConfig as ipcSaveSafetyConfig,
} from "./ipc/safety";
import { listOnboardingPlatforms, applyOnboarding } from "./ipc/onboarding";
import {
  listRecipes,
  getRecipe,
  listRecipeCategories,
  toggleRecipeFavorite,
  recordRecipeRecent,
  applyRecipe,
  renderRecipeToml,
  preflightRecipe,
  previewRecipe,
  listRecipePieces,
  saveUserRecipe,
  forkRecipe,
  deleteUserRecipe,
  exportRecipeToml,
  importRecipeToml,
} from "./ipc/recipes";
import { listTemplates, writeTemplate } from "./ipc/templates";
import { listFixes, getFix, applyFix } from "./ipc/fixes";
import { sendMessage } from "./ipc/send";

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
    listConnectorOutputs: (name: string, limit?: number) => listConnectorOutputs(name, limit) as any,

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
    subscribeToEvents(callback) {
      let unlisten: (() => void) | undefined;
      listen<EventEntry>("event-fired", (e) => callback(e.payload))
        .then((u) => { unlisten = u; })
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
        console.warn("subscribe_canvas failed:", e);
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
    setDisguiseProfile: (appName: string, iconId: string) =>
      ipcSetDisguiseProfile(appName, iconId),
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
