/**
 * Typed IPC wrappers for diagnostic operations.
 */
import { invoke } from "@tauri-apps/api/core";
import type { Report } from "@springtale/types";

export async function runDiagnostics(): Promise<Report> {
  return invoke<Report>("run_diagnostics");
}
