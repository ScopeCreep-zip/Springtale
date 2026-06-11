/**
 * Typed IPC wrappers for Phase B executions-log commands.
 *
 * Privacy-default by design — the backend returns sizes-only rows.
 * Content retention is opt-in (Phase C) and exposed through the
 * `*_blob_ref` fields, never inlined.
 */

import type {
  ExecutionFilterInput,
  ExecutionInfo,
  ExecutionStepInfo,
} from "@springtale/ui/dashboard/types";
import { invoke } from "@tauri-apps/api/core";

export async function listExecutions(filter: ExecutionFilterInput): Promise<ExecutionInfo[]> {
  return invoke<ExecutionInfo[]>("list_executions", { filter });
}

export async function getExecutionSteps(executionId: string): Promise<ExecutionStepInfo[]> {
  return invoke<ExecutionStepInfo[]>("get_execution_steps", { executionId });
}
