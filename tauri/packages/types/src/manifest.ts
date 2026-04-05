/**
 * Matches: springtale-connector/src/manifest/types.rs — TriggerDecl, ActionDecl
 * Used by the visual rule builder to enumerate available triggers and actions.
 */

export interface TriggerDecl {
  name: string;
  description: string;
  schema: Record<string, unknown> | null;
}

export interface ActionDecl {
  name: string;
  description: string;
  input_schema: Record<string, unknown> | null;
  output_schema: Record<string, unknown> | null;
}

export interface ConnectorSchema {
  name: string;
  version: string;
  description: string;
  triggers: TriggerDecl[];
  actions: ActionDecl[];
}
