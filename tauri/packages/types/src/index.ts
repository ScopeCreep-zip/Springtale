/**
 * Shared TypeScript types matching Rust schema types.
 *
 * These interfaces MUST stay in sync with the corresponding Rust structs
 * in springtale-store/src/schema/ and springtale-core/src/rule/types.rs.
 *
 * G3 — types under `./generated/` are produced automatically by ts-rs
 * derives in the cooperation crate. Regenerate after changing Rust
 * source: `cargo test -p springtale-cooperation --lib gossip::types`.
 */

// G3 — auto-generated from `springtale-cooperation` (do not edit by hand).
export type {
  FormationDelta,
  FormationOutcome,
  FormationStatus,
  FormationView,
} from "./generated";

export type { Connector } from "./connector";
export type { Rule, RuleStatus, RuleId } from "./rule";
export type { EventEntry, EventFilter } from "./event";
export type { AuditEntry, AuditFilter } from "./audit";
export type { Session } from "./session";
export type { TriggerDecl, ActionDecl, ConnectorSchema } from "./manifest";
export type { AvailableConnector, ConfigSchema, ConfigSchemaProperty } from "./available-connector";
export type { AgentState } from "./agent-state";
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
export type {
  Severity,
  Check,
  Report,
  FixGuide,
  FixOutcome,
  FormField,
  PlatformForm,
  ApplyReport,
  TemplateFile,
  Template,
  WriteReport,
  SendRequest,
  SendOutcome,
} from "./operations";
