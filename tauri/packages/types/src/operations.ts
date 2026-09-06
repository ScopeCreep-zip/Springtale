/**
 * Operation types matching Rust structs in springtale-runtime/src/operations/.
 *
 * Field names use snake_case to match Rust's default serde serialization.
 * These types cross the Tauri IPC boundary — any mismatch causes silent
 * deserialization failures.
 */

// --- Diagnostics (diagnostics.rs) ---

/** Severity of a diagnostic finding. Matches `Severity` enum with `#[serde(rename_all = "lowercase")]`. */
export type Severity = "ok" | "warn" | "fail";

/** One diagnostic finding. Matches `Check` struct. */
export interface Check {
  id: string;
  label: string;
  severity: Severity;
  detail: string | null;
  fix_hint: string | null;
}

/** Aggregated result of running all diagnostics. Matches `Report` struct. */
export interface Report {
  checks: Check[];
}

// --- Error fixes (error_fixes.rs) ---

/** Static guidance for a single error ID. Matches `FixGuide` struct. */
export interface FixGuide {
  id: string;
  title: string;
  causes: string[];
  suggestions: string[];
  has_auto_fix: boolean;
}

/** Result of an attempted automated fix. Matches `FixOutcome` struct. */
export interface FixOutcome {
  id: string;
  success: boolean;
  messages: string[];
}

// --- Onboarding (onboarding.rs) ---

/** A single field the user must fill in for a platform. Matches `FormField` struct. */
export interface FormField {
  name: string;
  label: string;
  description: string;
  secret: boolean;
  default: string | null;
  required: boolean;
  validation: string | null;
}

/** One platform the onboarding wizard knows how to set up. Matches `PlatformForm` struct. */
export interface PlatformForm {
  id: string;
  config_key: string;
  label: string;
  description: string;
  setup_help: string;
  fields: FormField[];
}

/** Summary of a successful `apply_platform` call. Matches `ApplyReport` struct. */
export interface ApplyReport {
  platform: string;
  stored_key: string;
  fields_stored: string[];
}

// --- Cross-channel (cross_channel.rs) ---

/** Request to send a message through a specific connector. Matches `SendRequest` struct. */
export interface SendRequest {
  connector: string;
  channel_id: string;
  text: string;
}

/** Outcome returned after sending a message. Matches `SendOutcome` struct. */
export interface SendOutcome {
  connector: string;
  channel_id: string;
  message: string;
  output: unknown;
}
