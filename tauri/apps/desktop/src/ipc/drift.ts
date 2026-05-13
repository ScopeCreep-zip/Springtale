/**
 * Typed IPC wrappers for Phase C drift detection.
 *
 * The backend reads recent rows from the executions log and
 * computes latency / success-rate / refusal-rate trends. Returns
 * the privacy-shaped DriftReport (sizes / counts / enum tags).
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  DriftFilterInput,
  DriftReport,
} from "@springtale/ui/dashboard/types";

export async function getRecipeDrift(
  recipeId: string,
  filter: DriftFilterInput,
): Promise<DriftReport> {
  return invoke<DriftReport>("get_recipe_drift", { recipeId, filter });
}

export async function getRuleDrift(
  filter: DriftFilterInput,
): Promise<DriftReport> {
  return invoke<DriftReport>("get_rule_drift", { filter });
}
