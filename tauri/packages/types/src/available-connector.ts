/**
 * Matches: springtale-runtime/src/operations/connectors.rs — AvailableConnectorInfo
 *
 * Static descriptor for a connector that CAN be installed (from factory registry).
 * Includes config schema, trigger/action declarations — all available without
 * a live connector instance (n8n descriptor pattern).
 */
import type { TriggerDecl, ActionDecl } from "./manifest";

/** JSON Schema property descriptor for a single config field. */
export interface ConfigSchemaProperty {
  type: string;
  description?: string;
  default?: unknown;
  enum?: string[];
  items?: { type: string };
  additionalProperties?: { type: string };
  /** When true, the field contains a secret (API key, token, password).
   *  Frontend renders as a masked password input. */
  "x-secret"?: boolean;
}

/** JSON Schema describing a connector's config struct. */
export interface ConfigSchema {
  type: "object";
  properties: Record<string, ConfigSchemaProperty>;
  required?: string[];
}

/** Available connector descriptor — static discovery without instantiation. */
export interface AvailableConnector {
  /** Connector name (e.g., "connector-telegram"). */
  name: string;
  /** Config key for TOML/config store (e.g., "telegram"). */
  config_key: string;
  /** Whether config is required to instantiate. */
  requires_config: boolean;
  /** Whether this connector is currently loaded in the registry. */
  loaded: boolean;
  /** JSON Schema describing the connector's config struct. */
  config_schema?: ConfigSchema;
  /** Static trigger declarations — what events this connector can emit. */
  triggers: TriggerDecl[];
  /** Static action declarations — what actions this connector can perform. */
  actions: ActionDecl[];
}
