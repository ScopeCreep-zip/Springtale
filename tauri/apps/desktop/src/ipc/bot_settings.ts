/**
 * Bot settings IPC (plan 6.3) — persona, context window, AI tool policy.
 *
 * These go through the runtime operation, not a raw config write: the
 * operation validates the tool allow-list against the connector registry
 * and hot-swaps the live settings, so a change lands on the next message
 * instead of the next restart.
 *
 * Keep in sync with `tauri/apps/desktop/src-tauri/src/commands/bot_settings.rs`.
 */

import type { BotSettingsValue } from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";

export async function getBotSettings(): Promise<BotSettingsValue> {
  return invoke<BotSettingsValue>("get_bot_settings");
}

export async function saveBotSettings(settings: BotSettingsValue): Promise<void> {
  await invoke("save_bot_settings", { settings });
}
