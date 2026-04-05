/**
 * Matches: springtale-store/src/schema/audit.rs — AuditEntry, AuditFilter
 */
export interface AuditEntry {
  id: string; // UUID
  timestamp: string; // ISO 8601
  connector_name: string;
  action_type: string;
  action_summary: string;
  verdict: string;
  verdict_reason: string;
  result: string;
}

export interface AuditFilter {
  connector_name?: string;
  after?: string;
  before?: string;
  verdict?: string;
  limit?: number;
  offset?: number;
}
