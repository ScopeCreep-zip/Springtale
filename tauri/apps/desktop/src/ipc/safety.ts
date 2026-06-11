/**
 * Safety IPC wrappers — typed commands for safety settings.
 *
 * Safety config is stored in SQLite (not vault) — it must
 * load before vault unlock so the app starts disguised.
 *
 * G5d note: the `SafetyConfig` shape mirrors the Rust
 * `SafetyConfigRow` exactly. Earlier versions of this file dropped
 * the four new disguise fields, which would silently wipe a
 * survivor's persisted disguise state on the next save round-trip.
 * Keep this in sync with `tauri/apps/desktop/src-tauri/src/commands/safety.rs`.
 */
import { invoke } from "@tauri-apps/api/core";

export interface SafetyConfig {
  window_title: string;
  auto_lock_minutes: number;
  content_protected: boolean;
  quick_hide_shortcut: string;
  disguise_app_name: string;
  disguise_icon_id: string;
  disguise_active: boolean;
  panic_tap_count: number;
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

/** G5d — flip just disguise_active without re-sending the full config. */
export async function setDisguiseActive(active: boolean): Promise<boolean> {
  return invoke<boolean>("set_disguise_active", { active });
}

/** G5d — atomically update the disguise profile (app name + icon id). */
export async function setDisguiseProfile(appName: string, iconId: string): Promise<void> {
  return invoke("set_disguise_profile", { appName, iconId });
}

/** G5d — set the panic-tap threshold; backend rejects values out of [0, 10]. */
export async function setPanicTapCount(count: number): Promise<number> {
  return invoke<number>("set_panic_tap_count", { count });
}

/**
 * G5f — apply the persisted disguise state to the visible shell.
 * Returns the title that was actually applied. Idempotent: callable
 * after any disguise-related write to snap the shell to the
 * backend's current view of the config.
 */
export async function applyDisguiseToShell(): Promise<string> {
  return invoke<string>("apply_disguise_to_shell");
}

/**
 * G5g — apply the persisted content_protected flag to the window.
 * Blocks screenshots / screen recording on macOS + Windows; no-op
 * on Linux. Returns the bool that was applied.
 */
export async function applyContentProtection(): Promise<boolean> {
  return invoke<boolean>("apply_content_protection");
}

/**
 * G5f — apply the persisted disguise state to the tray icon. Swaps
 * both the icon image (resolved from `icons/disguise/{id}.png`) and
 * the tooltip text to match `disguise_app_name`. Idempotent.
 *
 * Returns the tooltip that was applied (the disguise app name when
 * active, "Springtale" otherwise). Soft-fails when the tray is
 * unsupported on the platform — the rest of the disguise chain
 * (window title + content protection) still applies.
 */
export async function applyDisguiseToTray(): Promise<string> {
  return invoke<string>("apply_disguise_to_tray");
}

/**
 * G5g — register the persisted `quick_hide_shortcut` as an OS-wide
 * global hotkey. On press the backend hides the main window and
 * emits a "quick-hide" event; the frontend listens for that event
 * to run the existing lock-vault teardown.
 *
 * Returns the shortcut string that was actually applied. Call again
 * after the user changes the shortcut in the Safety panel — the
 * backend handles the unregister/register swap atomically.
 */
export async function applyQuickHideShortcut(): Promise<string> {
  return invoke<string>("apply_quick_hide_shortcut");
}
