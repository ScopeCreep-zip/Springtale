/**
 * Quick-hide — instantly minimizes the app window.
 *
 * Per ARCHITECTURE.md §2.8: "Configurable gesture instantly
 * minimizes to last-used app."
 *
 * Uses Tauri's window API + global shortcut plugin. No confirmation
 * dialog — milliseconds matter when someone is looking over your shoulder.
 */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";

export async function quickHide() {
  const win = getCurrentWindow();
  await win.minimize();
}

const DEFAULT_SHORTCUT = "Ctrl+Shift+H";

/**
 * Register the quick-hide global shortcut.
 * Returns an unregister function for cleanup.
 */
export async function registerQuickHide(
  shortcut: string = DEFAULT_SHORTCUT,
): Promise<() => Promise<void>> {
  await register(shortcut, async (event) => {
    if (event.state === "Pressed") {
      await quickHide();
    }
  });

  return async () => {
    await unregister(shortcut);
  };
}
