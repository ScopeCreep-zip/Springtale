/**
 * Matches: springtale-store/src/schema/events.rs — EventEntry, EventFilter
 */
export interface EventEntry {
  id: string; // UUID
  connector_name: string;
  trigger_type: string;
  timestamp: string; // ISO 8601
  action_taken: string;
}

export interface EventFilter {
  connector_name?: string;
  trigger_type?: string;
  after?: string; // ISO 8601
  before?: string; // ISO 8601
  limit?: number;
  offset?: number;
}
