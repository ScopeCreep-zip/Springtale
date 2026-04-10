/**
 * Web data provider — wraps HTTP client + SSE into DataProvider interface.
 *
 * Thin adapter: delegates to existing api/ modules, unwraps HTTP
 * response envelopes ({ connectors: [...] } → [...]).
 */
import type { DataProvider } from "@springtale/ui";
import type { ConnectorSchema, EventEntry, AvailableConnector } from "@springtale/types";
import type { RuleSummary, FormationInfo } from "@springtale/ui";
import { get, post, put, del, getBaseUrl, getToken } from "./api/client";
import { subscribeToEvents } from "./api/events";
import { getCanvasState, subscribeToCanvasUpdates } from "./api/canvas";

export function createWebProvider(): DataProvider {
  return {
    // Connectors
    async listConnectors() {
      const data = await get<{ connectors: Array<{ name: string; enabled: boolean }> }>("/connectors");
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
    async enableConnector(name) { await post(`/connectors/${name}/enable`); },
    async disableConnector(name) { await post(`/connectors/${name}/disable`); },
    async removeConnector(name) { await del(`/connectors/${name}`); },
    async removeConnectorCascade(name) {
      const data = await del<{ removed: string; rules_deleted: string[] }>(`/connectors/${name}/cascade`);
      return data.rules_deleted ?? [];
    },
    async getConnectorConfig(name) {
      const data = await get<{ config: unknown }>(`/connectors/${name}/config`);
      return data.config;
    },
    async listConnectorOutputs(name, limit = 20) {
      const data = await get<{ outputs: unknown[] }>(`/connectors/${name}/outputs?limit=${limit}`);
      return data.outputs as any;
    },

    // Rules
    async listRules() {
      const data = await get<{ rules: RuleSummary[] }>("/rules");
      return data.rules;
    },
    async createRule(rule) { return (await post<{ id: string }>("/rules", rule)).id; },
    async toggleRule(id, enabled) { await post(`/rules/${id}/toggle`, { enabled }); },
    async deleteRule(id) { await del(`/rules/${id}`); },
    async updateRule(id, rule) { await put(`/rules/${id}`, rule); },
    async runRule(id) { return post<{ matched: boolean }>(`/rules/${id}/run`); },
    async parseRuleFromIntent(intent) {
      const data = await post<{ rule: Record<string, unknown> }>("/rules/parse", { intent });
      return data.rule;
    },
    async createConnectorRule(rule) {
      return (await post<{ id: string }>("/rules/connector", rule)).id;
    },
    async listRulesForConnector(connectorName) {
      const data = await get<{ rules: import("@springtale/types").RuleSummary[] }>(`/rules/connector/${connectorName}`);
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
    subscribeToEvents(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToEvents(getBaseUrl(), token, callback);
    },

    // Formations
    async listFormations() {
      const data = await get<{ formations: FormationInfo[] }>("/formations");
      return data.formations ?? [];
    },
    async createFormation(name, intent, connectors) {
      return (await post<{ id: string }>("/formations", { name, intent, connectors })).id;
    },
    async deployFormation(id) { await post(`/formations/${id}/deploy`); },
    async pauseFormation(id) { await post(`/formations/${id}/pause`); },
    async resumeFormation(id) { await post(`/formations/${id}/resume`); },
    async dissolveFormation(id) { await post(`/formations/${id}/dissolve`); },
    async updateFormationIntent(id, intent) { await put(`/formations/${id}/intent`, { intent }); },
    async addFormationMember(formationId, connectorName) { await post(`/formations/${formationId}/members`, { connector_name: connectorName }); },
    async listIntents() { return (await get<{ intents: Array<{ value: string; label: string }> }>("/formations/intents")).intents ?? []; },
    async deployTeam(team) { return post("/formations/deploy-team", team); },
    async cycleFormationIntent(id) { return (await post<{ intent: string }>(`/formations/${id}/cycle-intent`)).intent; },
    async cycleFormationAutonomy(id) { return (await post<{ level: string }>(`/formations/${id}/cycle-autonomy`)).level; },

    // Config
    async getConfig(key) { return get(`/config/${key}`); },
    async setConfig(key, value) { await put(`/config/${key}`, value); },
    async listConfig() { return (await get<{ config: Array<[string, unknown]> }>("/config")).config; },
    async setAiAdapter(config) { await post("/config/ai", config); },
    async setConnectorConfig(name, config) { await post(`/config/connector/${name}`, config); },
    async configureAiAdapter(target, config) { await post("/config/ai/configure", { target, config }); },
    async upsertConnectorConfig(name, config) { return (await post<{ is_new: boolean }>(`/connectors/${name}/upsert-config`, config)).is_new; },
    async toggleFormationGuard(formationId) { return (await post<{ enabled: boolean }>(`/formations/${formationId}/toggle-guard`)).enabled; },

    // Agent state + autonomy
    async listAgentStates() {
      const data = await get<{ agents: import("@springtale/types").AgentState[] }>("/agents/states");
      return data.agents ?? [];
    },
    async getAutonomy(name) { return (await get<{ level: string }>(`/agents/${name}/autonomy`)).level; },
    async setAutonomy(name, level) { await put(`/agents/${name}/autonomy`, { level }); },
    async stepAutonomy(name, direction) { return (await post<{ level: string }>(`/agents/${name}/autonomy/step`, { direction })).level; },

    // Trusted authors
    async listAuthors() {
      const data = await get<{ authors: Array<{ name: string; pubkey: string }> }>("/authors");
      return data.authors ?? [];
    },
    async addAuthor(name, pubkey) { await post(`/authors/${name}`, { pubkey }); },
    async removeAuthor(name) { await del(`/authors/${name}`); },

    // Data
    async exportData() { return post("/data/export"); },

    // Memory
    async auditMemory() { return post("/memory/audit"); },
    async compactMemory(maxEntries) { await post("/memory/compact", { max_entries: maxEntries }); },

    // Canvas
    async getCanvasState() { return getCanvasState(); },
    subscribeToCanvasUpdates(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToCanvasUpdates(getBaseUrl(), token, callback);
    },
  };
}
