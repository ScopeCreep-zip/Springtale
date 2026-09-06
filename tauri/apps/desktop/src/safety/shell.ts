/**
 * Push the daemon's safety config onto the OS shell.
 *
 * The config itself lives in the daemon's database and is read with
 * `GET /safety` through the provider. Only the *effects* are local:
 * window title, tray icon, screen-capture protection, the global
 * quick-hide hotkey and the auto-lock timer. Every apply is idempotent,
 * so this runs on unlock and again after each Safety-panel save.
 *
 * Ordering matters for a survivor under duress: the visible chrome
 * (title, tray) goes first so the disguise lands on the first frame,
 * before the slower hotkey registration.
 */
import type { SafetyConfig } from "@springtale/ui";
import { resetAutoLock } from "../ipc/autolock";
import {
  applyContentProtection,
  applyDisguiseToShell,
  applyDisguiseToTray,
  applyQuickHideShortcut,
} from "../ipc/safety";

export async function applySafetyToShell(config: SafetyConfig): Promise<void> {
  await applyDisguiseToShell(config.disguise_active, config.disguise_app_name, config.window_title);
  // Soft-fails where the platform has no tray; the rest still applies.
  await applyDisguiseToTray(
    config.disguise_active,
    config.disguise_app_name,
    config.disguise_icon_id,
  );
  await applyContentProtection(config.content_protected);
  // A global shortcut is a convenience, not a requirement — the backend
  // falls back rather than failing when the combo is taken.
  await applyQuickHideShortcut(config.quick_hide_shortcut);
  await resetAutoLock(config.auto_lock_minutes);
}
