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

export type { AgentState } from "./agent-state";
/**
 * The generated API contract.
 *
 * `src/api.ts` is produced by `pnpm --filter @springtale/types build`
 * (openapi-typescript over `openapi.json`, which springtaled derives
 * from its own handlers via `--dump-openapi`). Do not edit it by hand:
 * a wire shape changes in Rust, and the TypeScript follows.
 */
export type { components, operations, paths } from "./api";
export type { AuditEntry, AuditFilter } from "./audit";
export type { AvailableConnector, ConfigSchema, ConfigSchemaProperty } from "./available-connector";
export type { CanvasBlock, CanvasState, CanvasUpdate, StatusState } from "./canvas";
export type { Connector } from "./connector";
export type { EventEntry, EventFilter } from "./event";
export type {
  AgentHealth,
  DynamicRole,
  FormationDetail,
  FormationMember,
  FormationSummary,
  IntentPattern,
  MomentumTier,
} from "./formation";
// G3 — auto-generated from `springtale-cooperation` (do not edit by hand).
export type {
  FormationDelta,
  FormationOutcome,
  FormationStatus,
  FormationView,
} from "./generated";
export type { ActionDecl, ConnectorSchema, TriggerDecl } from "./manifest";
export type {
  ApplyReport,
  Check,
  FixGuide,
  FixOutcome,
  FormField,
  PlatformForm,
  Report,
  SendOutcome,
  SendRequest,
  Severity,
} from "./operations";
export type { Rule, RuleId, RuleStatus } from "./rule";
export type { Session } from "./session";

/** Every response/request schema the daemon declares, by name. */
export type ApiSchemas = import("./api").components["schemas"];
