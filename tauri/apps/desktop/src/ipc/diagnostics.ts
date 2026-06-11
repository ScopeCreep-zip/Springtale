/**
 * Typed IPC wrappers for diagnostic operations.
 */

import type { Report } from "@springtale/types";
import { invoke } from "@tauri-apps/api/core";

export async function runDiagnostics(): Promise<Report> {
  return invoke<Report>("run_diagnostics");
}
