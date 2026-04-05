/**
 * Matches: springtale-store/src/schema/connectors.rs — ConnectorRow
 */
export interface Connector {
  name: string;
  version: string;
  author: string;
  description: string;
  manifest_json: string;
  enabled: boolean;
  installed_at: string; // ISO 8601
}
