/**
 * Typed IPC wrappers for error-fix operations.
 */

import type { FixGuide, FixOutcome } from "@springtale/types";
import { invoke } from "@tauri-apps/api/core";

export async function listFixes(): Promise<FixGuide[]> {
  return invoke<FixGuide[]>("list_fixes");
}

export async function getFix(id: string): Promise<FixGuide> {
  return invoke<FixGuide>("get_fix", { id });
}

export async function applyFix(id: string): Promise<FixOutcome> {
  return invoke<FixOutcome>("apply_fix", { id });
}
