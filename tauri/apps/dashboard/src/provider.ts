/**
 * Web data provider — wraps HTTP client + SSE into DataProvider interface.
 *
 * Thin adapter: delegates to existing api/ modules, unwraps HTTP
 * response envelopes ({ connectors: [...] } → [...]).
 *
 * SSE subscriptions gracefully handle missing auth token — return
 * no-op unsubscribe when getToken() is empty. After the user
 * configures auth in Settings, call dashboard.resubscribe().
 */
import type { DataProvider } from "@springtale/ui";
import type { ConnectorSchema, EventEntry } from "@springtale/types";
import type { RuleSummary, FormationInfo } from "@springtale/ui";
import { get, post, del, getBaseUrl, getToken } from "./api/client";
import { subscribeToEvents } from "./api/events";
import { getCanvasState, subscribeToCanvasUpdates } from "./api/canvas";

export function createWebProvider(): DataProvider {
  return {
    async listConnectors() {
      const data = await get<{ connectors: Array<{ name: string; enabled: boolean }> }>("/connectors");
      return data.connectors;
    },

    async getConnectorSchemas() {
      const data = await get<{ manifests: ConnectorSchema[] }>("/connectors/schemas");
      return data.manifests ?? [];
    },

    async listRules() {
      const data = await get<{ rules: RuleSummary[] }>("/rules");
      return data.rules;
    },

    async createRule(rule) {
      const data = await post<{ id: string }>("/rules", rule);
      return data.id;
    },

    async toggleRule(id, enabled) {
      await post(`/rules/${id}/toggle`, { enabled });
    },

    async deleteRule(id) {
      await del(`/rules/${id}`);
    },

    async listEvents(limit = 50) {
      const data = await get<{ events: EventEntry[] }>(`/events?limit=${limit}`);
      return data.events ?? [];
    },

    subscribeToEvents(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToEvents(getBaseUrl(), token, callback);
    },

    async listFormations() {
      const data = await get<{ formations: FormationInfo[] }>("/formations");
      return data.formations ?? [];
    },

    async createFormation(name, intent, connectors) {
      const data = await post<{ id: string }>("/formations", { name, intent, connectors });
      return data.id;
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

    async getCanvasState() {
      return getCanvasState();
    },

    subscribeToCanvasUpdates(callback) {
      const token = getToken();
      if (!token) return () => {};
      return subscribeToCanvasUpdates(getBaseUrl(), token, callback);
    },
  };
}
