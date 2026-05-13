/**
 * Typed IPC wrapper for the authoring-time selector picker.
 *
 * The Rust command opens a Tauri webview at `url`, injects the
 * bundled picker.js overlay, and resolves with the picked CSS
 * selector (or null when the user closes the window without
 * picking). No chromiumoxide involvement — picking is a UI tool,
 * not a headless feature.
 */
import { invoke } from "@tauri-apps/api/core";

export async function openSelectorPicker(
  url: string,
  hostAllowlist: string[],
): Promise<string | null> {
  return invoke<string | null>("open_selector_picker", {
    url,
    hostAllowlist,
  });
}
