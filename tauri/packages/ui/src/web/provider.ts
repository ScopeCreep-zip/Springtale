/**
 * Web data provider — wraps HTTP client + SSE into DataProvider interface.
 *
 * Thin adapter: delegates to existing api/ modules, unwraps HTTP
 * response envelopes ({ connectors: [...] } → [...]).
 */

import type {
  ApplyReport,
  AvailableConnector,
  ConnectorSchema,
  EventEntry,
  FixGuide,
  FixOutcome,
  PlatformForm,
  Report,
  SendOutcome,
} from "@springtale/types";
import type {
  ApprovalInfo,
  ConnectorOutput,
  DataProvider,
  DriftReport,
  ExecutionInfo,
  ExecutionStepInfo,
  FormationDetail,
  FormationInfo,
  RuleSummary,
  TestStepReport,
  WorkspaceInfo,
} from "../dashboard/types";
import { getCanvasState, subscribeToCanvasUpdates } from "./api/canvas";
import { sendChatMessage, subscribeToChat } from "./api/chat";
import { del, get, getBaseUrl, getToken, post, put } from "./api/client";
import { getUtteranceDefs, subscribeToCooperationEvents } from "./api/cooperation";
import { subscribeToEvents } from "./api/events";
import * as onboard from "./api/onboard";
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
} from "./api/recipes";

/** `?k=v&…` from the defined entries of a flat filter object. */
function queryString(params: object): string {
  const qs = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null) qs.set(key, String(value));
  }
  const encoded = qs.toString();
  return encoded ? `?${encoded}` : "";
}

export function createWebProvider(): DataProvider {
  return {
    // Connectors
    async listConnectors() {
      const data = await get<{ connectors: Array<{ name: string; enabled: boolean }> }>(
        "/connectors",
      );
      return data.connectors;
    },
    async listAvailableConnectors() {
      const data = await get<{ available: AvailableConnector[] }>("/connectors/available");
      return data.available ?? [];
    },
    async setupConnector(name, config) {
      const data = await post<{ name: string }>("/connectors/setup", { name, config });
      return data.name;
    },
    async getConnectorSchemas() {
      const data = await get<{ manifests: ConnectorSchema[] }>("/connectors/schemas");
      return data.manifests ?? [];
    },
    async enableConnector(name) {
      await post(`/connectors/${name}/enable`);
    },
    async disableConnector(name) {
      await post(`/connectors/${name}/disable`);
    },
    async reloadConnector(name) {
      await post(`/connectors/${name}/reload`);
    },
    async removeConnector(name) {
      await del(`/connectors/${name}`);
    },
    async removeConnectorCascade(name) {
      const data = await del<{ removed: string; rules_deleted: string[] }>(
        `/connectors/${name}/cascade`,
      );
      return data.rules_deleted ?? [];
    },
    async getConnectorConfig(name) {
      const data = await get<{ config: unknown }>(`/connectors/${name}/config`);
      return data.config;
    },
    async listConnectorOutputs(name, limit = 20) {
      const data = await get<{ outputs: ConnectorOutput[] }>(
        `/connectors/${name}/outputs?limit=${limit}`,
      );
      return data.outputs;
    },

    // Rules
    async listRules() {
      const data = await get<{ rules: RuleSummary[] }>("/rules");
      return data.rules;
    },
    async createRule(rule) {
      return (await post<{ id: string }>("/rules", rule)).id;
    },
    async toggleRule(id, enabled) {
      await post(`/rules/${id}/toggle`, { enabled });
    },
    async deleteRule(id) {
      await del(`/rules/${id}`);
    },
    async updateRule(id, rule) {
      await put(`/rules/${id}`, rule);
    },
    async runRule(id) {
      return post<{ matched: boolean }>(`/rules/${id}/run`);
    },
    async parseRuleFromIntent(intent) {
      const data = await post<{ rule: Record<string, unknown> }>("/rules/parse", { intent });
      return data.rule;
    },
    async getRuleSchema() {
      return get("/rules/schema");
    },
    async createConnectorRule(rule) {
      return (await post<{ id: string }>("/rules/connector", rule)).id;
    },
    async listRulesForConnector(connectorName) {
      const data = await get<{ rules: import("../dashboard/types").RuleSummary[] }>(
        `/rules/connector/${connectorName}`,
      );
      return data.rules ?? [];
    },
    async testConnector(connectorName) {
      return post(`/connectors/${connectorName}/test`);
    },
    async reassignRuleConnector(id, newConnector) {
      await post(`/rules/${id}/reassign`, { new_connector: newConnector });
    },

    // Events
    async listEvents(limit = 50) {
      const data = await get<{ events: EventEntry[] }>(`/events?limit=${limit}`);
      return data.events ?? [];
    },

    // Phase B — executions log. Same routes the desktop IPC
    // commands mirror (plan 2.5).
    async listExecutions(filter) {
      return get<ExecutionInfo[]>(`/executions${queryString(filter)}`);
    },
    async getExecutionSteps(executionId) {
      return get<ExecutionStepInfo[]>(`/executions/${encodeURIComponent(executionId)}/steps`);
    },
    // Selector picker requires a desktop webview — there's no
    // safe way to overlay a third-party site inside a hosted
    // dashboard. Web users type selectors manually.
    async openSelectorPicker(_url, _hostAllowlist) {
      throw new Error(
        "The selector picker is desktop-only: the web dashboard cannot overlay a third-party site. Type the selector manually.",
      );
    },

    // Phase C — Test This Step + drift.
    async testRecipeStep(recipeId, inputs, ruleIndex, stepIndex) {
      return post<TestStepReport>(`/recipes/${encodeURIComponent(recipeId)}/test-step`, {
        inputs,
        rule_index: ruleIndex,
        step_index: stepIndex,
      });
    },
    async getRecipeDrift(recipeId, filter) {
      return get<DriftReport>(
        `/drift/recipe/${encodeURIComponent(recipeId)}${queryString(filter)}`,
      );
    },

    // D1 — External-workspace directory.
    async listWorkspaces(formationId, connectorFilter) {
      return get<WorkspaceInfo[]>(
        `/workspaces${queryString({ formation_id: formationId, connector: connectorFilter })}`,
      );
    },
    async scanWorkspaces(formationId, connectorName) {
      return post<WorkspaceInfo[]>("/workspaces/scan", {
        formation_id: formationId,
        connector_name: connectorName,
      });
    },
    async deleteWorkspace(formationId, workspaceKey) {
      await del(
        `/workspaces${queryString({ formation_id: formationId, workspace_key: workspaceKey })}`,
      );
    },
    async upsertWorkspaceManual(formationId, workspaceKey, displayName, connectorName, kind) {
      await post("/workspaces", {
        formation_id: formationId,
        workspace_key: workspaceKey,
        display_name: displayName,
        connector_name: connectorName,
        kind,
      });
    },
    async previewOnboardUrl(connectorName, config, payload) {
      const data = await post<{ url: string }>("/workspaces/onboard-url", {
        connector_name: connectorName,
        config,
        payload,
      });
      return data.url;
    },
    // Track D — onboard stream is SSE over a POST (config in the
    // body); see api/onboard.ts.
    async startOnboardStream(sessionId, connectorName, config, payload) {
      await onboard.startOnboardStream(sessionId, connectorName, config, payload);
    },
    async cancelOnboardStream(sessionId) {
      onboard.cancelOnboardStream(sessionId);
    },
    async subscribeToChatDiscovered(callback) {
      return onboard.subscribeToChatDiscovered(callback);
    },
    // Plan 6.7 — chat-gate approval queue.
    async listApprovals() {
      const r = await get<{ pending: ApprovalInfo[] }>("/approvals");
      return r.pending;
    },
    async resolveApproval(id, approve) {
      await post(`/approvals/${encodeURIComponent(id)}`, {
        decision: approve ? "approve" : "deny",
      });
    },
    subscribeToEvents(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToEvents(getBaseUrl(), callback);
    },

    // In-app chat (W5)
    async sendChatMessage(text, session) {
      const token = getToken();
      await sendChatMessage(getBaseUrl(), token ?? "", text, session);
    },
    subscribeToChat(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToChat(getBaseUrl(), callback);
    },

    // Formations
    async getFormation(id: string) {
      return get<FormationDetail>(`/formations/${id}`);
    },
    async listFormations() {
      const data = await get<{ formations: FormationInfo[] }>("/formations");
      return data.formations ?? [];
    },
    async createFormation(name, intent, connectors) {
      return (await post<{ id: string }>("/formations", { name, intent, connectors })).id;
    },
    async deployFormation(id) {
      await post(`/formations/${id}/deploy`);
    },
    async pauseFormation(id) {
      await post(`/formations/${id}/pause`);
    },
    async resumeFormation(id) {
      await post(`/formations/${id}/resume`);
    },
    async dissolveFormation(id) {
      await post(`/formations/${id}/dissolve`);
    },
    async rallyFormation(id) {
      await post(`/formations/${id}/rally`);
    },
    async runFormationCommand(id, commandId, params) {
      await post(`/formations/${id}/run-command`, { command_id: commandId, params });
    },
    async updateFormationIntent(id, intent) {
      await put(`/formations/${id}/intent`, { intent });
    },
    async addFormationMember(formationId, connectorName) {
      await post(`/formations/${formationId}/members`, { connector_name: connectorName });
    },
    async removeFormationMember(formationId, connectorName) {
      // DELETE with JSON body — use fetch directly since del() helper doesn't support body
      const response = await fetch(`${getBaseUrl()}/formations/${formationId}/members`, {
        method: "DELETE",
        headers: {
          Authorization: `Bearer ${getToken()}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ connector_name: connectorName }),
      });
      if (!response.ok) throw new Error(`API error: ${response.status}`);
    },
    async listIntents() {
      return (
        (await get<{ intents: Array<{ value: string; label: string }> }>("/formations/intents"))
          .intents ?? []
      );
    },
    async deployTeam(team) {
      return post("/formations/deploy-team", team);
    },
    async cycleFormationIntent(id) {
      return (await post<{ intent: string }>(`/formations/${id}/cycle-intent`)).intent;
    },
    async cycleFormationAutonomy(id) {
      return (await post<{ level: string }>(`/formations/${id}/cycle-autonomy`)).level;
    },
    // B11 thin-frontend APIs.
    async formationAvailableCommands(id) {
      return (
        await get<{ commands: Array<import("../dashboard/types").CommandDecl> }>(
          `/formations/${id}/commands`,
        )
      ).commands;
    },
    async formationEligibleMembers(id) {
      return (
        await get<{ members: Array<import("../dashboard/types").MemberRef> }>(
          `/formations/${id}/members/eligible`,
        )
      ).members;
    },

    // Config
    async getConfig(key) {
      return get(`/config/${key}`);
    },
    async setConfig(key, value) {
      await put(`/config/${key}`, value);
    },
    async listConfig() {
      return (await get<{ config: Array<[string, unknown]> }>("/config")).config;
    },
    async setAiAdapter(config) {
      await post("/config/ai", config);
    },
    async setConnectorConfig(name, config) {
      await post(`/config/connector/${name}`, config);
    },
    async configureAiAdapter(target, config) {
      await post("/config/ai/configure", { target, config });
    },
    async upsertConnectorConfig(name, config) {
      return (await post<{ is_new: boolean }>(`/connectors/${name}/upsert-config`, config)).is_new;
    },
    async toggleFormationGuard(formationId) {
      return (await post<{ enabled: boolean }>(`/formations/${formationId}/toggle-guard`)).enabled;
    },

    // Agent state + autonomy
    async listAgentStates() {
      const data = await get<{ agents: import("@springtale/types").AgentState[] }>(
        "/agents/states",
      );
      return data.agents ?? [];
    },
    async getAutonomy(name) {
      return (await get<{ level: string }>(`/agents/${name}/autonomy`)).level;
    },
    async setAutonomy(name, level) {
      await put(`/agents/${name}/autonomy`, { level });
    },
    async stepAutonomy(name, direction) {
      return (await post<{ level: string }>(`/agents/${name}/autonomy/step`, { direction })).level;
    },

    // Trusted authors
    async listAuthors() {
      const data = await get<{ authors: Array<{ name: string; pubkey: string }> }>("/authors");
      return data.authors ?? [];
    },
    async addAuthor(name, pubkey) {
      await post(`/authors/${name}`, { pubkey });
    },
    async removeAuthor(name) {
      await del(`/authors/${name}`);
    },

    // Data
    async exportData() {
      return post("/data/export");
    },

    // Canvas
    async getConnections() {
      return (
        (
          await get<{
            connections: Array<{
              a: string;
              b: string;
              pipes: Array<{ id: string; dir: 1 | -1; status: string }>;
            }>;
          }>("/canvas/connections")
        ).connections ?? []
      );
    },

    // Memory
    async auditMemory() {
      return post("/memory/audit");
    },
    async compactMemory(maxEntries) {
      await post("/memory/compact", { max_entries: maxEntries });
    },

    // Canvas
    async getCanvasState() {
      return getCanvasState();
    },
    async getUtteranceDefs() {
      return getUtteranceDefs();
    },
    subscribeToCanvasUpdates(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToCanvasUpdates(getBaseUrl(), callback);
    },

    subscribeToCooperationEvents(callback) {
      // Phase H — `cooperation` frames of the multiplexed /stream.
      const token = getToken();
      if (!token) return () => {};
      return subscribeToCooperationEvents(getBaseUrl(), callback);
    },

    // Safety — focused get/save against the dedicated safety table.
    async getSafetyConfig() {
      return await get<import("../dashboard/types").SafetyConfig>("/safety");
    },
    async saveSafetyConfig(config) {
      await put("/safety", config);
    },

    // Bot settings (plan 6.3) — one code path for desktop and web; the
    // desktop shell reaches the same daemon through the sidecar.
    async getBotSettings() {
      return await get<import("../colony/AppSettingsPanel").BotSettingsValue>("/bot/settings");
    },
    async saveBotSettings(settings) {
      await put("/bot/settings", settings);
    },

    // G5d — IPV duress surface (web).
    async setDisguiseActive(active: boolean) {
      const data = await post<{ disguise_active: boolean }>("/safety/disguise/active", { active });
      return data.disguise_active;
    },
    async setDisguiseProfile(appName: string, iconId: string) {
      await post("/safety/disguise/profile", { app_name: appName, icon_id: iconId });
    },
    async setPanicTapCount(count: number) {
      const data = await post<{ panic_tap_count: number }>("/safety/panic_tap_count", { count });
      return data.panic_tap_count;
    },

    // Diagnostics
    async runDiagnostics() {
      return get<Report>("/diagnostics");
    },

    // Onboarding
    async listOnboardingPlatforms() {
      const data = await get<{ platforms: PlatformForm[] }>("/onboarding/platforms");
      return data.platforms ?? [];
    },
    async applyOnboarding(platform, answers) {
      return post<ApplyReport>(`/onboarding/${encodeURIComponent(platform)}`, { answers });
    },

    // W1.B — Recipe library
    listRecipes,
    getRecipe,
    listRecipeCategories,
    toggleRecipeFavorite,
    recordRecipeRecent,
    // W1.C — Deploy + show-as-code
    applyRecipe,
    renderRecipeToml,
    // W1.D — Preflight
    preflightRecipe,
    // W2.C — Preview
    previewRecipe,
    // W2.D — Recipe pieces
    listRecipePieces,
    // W2.B — Recipe authoring
    saveUserRecipe,
    forkRecipe,
    deleteUserRecipe,
    exportRecipeToml,
    importRecipeToml,

    // Error fixes
    async listFixes() {
      const data = await get<{ guides: FixGuide[] }>("/fixes");
      return data.guides ?? [];
    },
    async getFix(id) {
      return get<FixGuide>(`/fixes/${id}`);
    },
    async applyFix(id) {
      return post<FixOutcome>(`/fixes/${id}/apply`);
    },

    // Cross-channel send
    async sendMessage(req) {
      return post<SendOutcome>("/send", req);
    },
  };
}
