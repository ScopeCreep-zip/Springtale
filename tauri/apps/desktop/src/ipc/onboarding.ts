/**
 * Typed IPC wrappers for onboarding operations.
 */
import { invoke } from "@tauri-apps/api/core";
import type { PlatformForm, ApplyReport } from "@springtale/types";

export async function listOnboardingPlatforms(): Promise<PlatformForm[]> {
  return invoke<PlatformForm[]>("list_onboarding_platforms");
}

export async function applyOnboarding(
  platform: string,
  answers: Record<string, string>,
): Promise<ApplyReport> {
  return invoke<ApplyReport>("apply_onboarding", { platform, answers });
}
