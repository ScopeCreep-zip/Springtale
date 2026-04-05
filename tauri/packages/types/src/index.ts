/**
 * Shared TypeScript types matching Rust schema types.
 *
 * These interfaces MUST stay in sync with the corresponding Rust structs
 * in springtale-store/src/schema/ and springtale-core/src/rule/types.rs.
 */

export type { Connector } from "./connector";
export type { Rule, RuleStatus, RuleId } from "./rule";
export type { EventEntry, EventFilter } from "./event";
export type { AuditEntry, AuditFilter } from "./audit";
export type { Session } from "./session";
export type { TriggerDecl, ActionDecl, ConnectorSchema } from "./manifest";
export type { CanvasBlock, CanvasState, CanvasUpdate, StatusState } from "./canvas";
export type {
  MomentumTier,
  AgentHealth,
  DynamicRole,
  FormationMember,
  FormationSummary,
  FormationDetail,
  IntentPattern,
} from "./formation";
