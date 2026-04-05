/**
 * Safety IPC wrappers — typed commands for safety settings.
 *
 * Safety config is stored in SQLite (not vault) — it must
 * load before vault unlock so the app starts disguised.
 */
import { invoke } from "@tauri-apps/api/core";

export interface SafetyConfig {
  window_title: string;
  auto_lock_minutes: number;
  content_protected: boolean;
  quick_hide_shortcut: string;
}

export async function getSafetyConfig(): Promise<SafetyConfig> {
  return invoke<SafetyConfig>("get_safety_config");
}

export async function saveSafetyConfig(config: SafetyConfig): Promise<void> {
  return invoke("save_safety_config", { config });
}

export async function setWindowTitle(title: string): Promise<void> {
  return invoke("set_window_title", { title });
}
