/**
 * OS-shell safety commands — the only safety surface that stays in Tauri.
 *
 * Reading and writing the safety config itself is the daemon's job
 * (`GET/PUT /safety`, reached through `DataProvider`). What survives here
 * is what only the desktop shell can do: window title, tray icon,
 * screen-capture protection and the OS-wide quick-hide hotkey. Each
 * command takes the values explicitly — the shell no longer reads the
 * store, so the caller passes the config it just fetched over HTTP.
 */
import { invoke } from "@tauri-apps/api/core";

/** Set the window title directly (disguise-independent). */
export async function setWindowTitle(title: string): Promise<void> {
  return invoke("set_window_title", { title });
}

/**
 * G5f — apply disguise state to the visible window chrome.
 * Returns the title that was actually applied.
 */
export async function applyDisguiseToShell(
  disguiseActive: boolean,
  disguiseAppName: string,
  windowTitle: string,
): Promise<string> {
  return invoke<string>("apply_disguise_to_shell", {
    disguiseActive,
    disguiseAppName,
    windowTitle,
  });
}

/**
 * G5f — apply disguise state to the tray icon + tooltip. Soft-fails on
 * platforms without a tray. Returns the tooltip that was applied.
 */
export async function applyDisguiseToTray(
  disguiseActive: boolean,
  disguiseAppName: string,
  disguiseIconId: string,
): Promise<string> {
  return invoke<string>("apply_disguise_to_tray", {
    disguiseActive,
    disguiseAppName,
    disguiseIconId,
  });
}

/**
 * G5g — block screenshots / screen recording on macOS + Windows
 * (no-op on Linux). Returns the flag that was applied.
 */
export async function applyContentProtection(isProtected: boolean): Promise<boolean> {
  return invoke<boolean>("apply_content_protection", { protected: isProtected });
}

/**
 * G5g — register `shortcut` as an OS-wide quick-hide hotkey. On press
 * the backend hides the window and emits "quick-hide". Returns the
 * shortcut that was actually applied (the backend falls back rather
 * than failing on a conflict).
 */
export async function applyQuickHideShortcut(shortcut: string): Promise<string> {
  return invoke<string>("apply_quick_hide_shortcut", { shortcut });
}
