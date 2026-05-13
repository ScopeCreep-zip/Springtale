/**
 * Phase B — authoring-time CSS selector picker.
 *
 * Renders a confirm dialog explaining what's about to happen, then
 * delegates to `db.provider.openSelectorPicker(url, allowlist)`.
 * The provider opens a Tauri webview on desktop, returns null on
 * web (no safe overlay path for hosted dashboards). Mirrors the
 * `MemberPickerOverlay` callback shape per the existing
 * thin-frontend pattern.
 */
import { Show, createSignal } from "solid-js";
import type { Component } from "solid-js";

import { useDashboard } from "../dashboard/context";

export interface SelectorPickerOverlayProps {
  /** URL to load in the picker webview. */
  initialUrl: string;
  /** Allow-list passed to picker.js. Empty = any host. */
  hostAllowlist?: string[];
  /** Called with the picked CSS selector. */
  onPicked: (selector: string) => void;
  /** Called when the user cancels (close, Escape, or web fallback). */
  onCancel: () => void;
}

export const SelectorPickerOverlay: Component<SelectorPickerOverlayProps> = (props) => {
  const db = useDashboard();
  const [working, setWorking] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [webFallback, setWebFallback] = createSignal(false);

  const openPicker = async () => {
    setWorking(true);
    setError(null);
    try {
      const picked = await db.provider.openSelectorPicker(
        props.initialUrl,
        props.hostAllowlist ?? [],
      );
      if (picked === null) {
        // Two cases land here: user cancelled, or web provider
        // returned null because picker requires desktop.
        setWebFallback(true);
        return;
      }
      props.onPicked(picked);
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  };

  return (
    <div class="mx-auto max-w-2xl rounded border-2 border-bark bg-soil-mid p-6">
      <p class="colony-text-md font-bold text-text-primary">
        Pick an element
      </p>
      <p class="colony-text-xs mt-2 text-text-secondary">
        Opens <span class="text-text-primary">{props.initialUrl}</span> in a
        picker window. Hover an element to highlight, click to copy its CSS
        selector back into this form. Press <kbd>Esc</kbd> to cancel.
      </p>
      <Show when={(props.hostAllowlist?.length ?? 0) > 0}>
        <p class="colony-text-3xs mt-2 text-text-dim">
          Recipe allow-list:{" "}
          {(props.hostAllowlist ?? []).join(", ") || "(none)"}
        </p>
      </Show>

      <Show when={error()}>
        <p class="colony-text-xs mt-4 text-status-warn">{error()}</p>
      </Show>

      <Show when={webFallback()}>
        <div class="mt-4 rounded border border-bark bg-soil-deep p-3">
          <p class="colony-text-xs text-status-warn">
            The selector picker requires the desktop app. On the web
            dashboard, type the CSS selector directly — use your
            browser's DevTools (Inspect → right-click the element →
            Copy → Selector) to grab one.
          </p>
        </div>
      </Show>

      <div class="mt-6 flex justify-end gap-2">
        <button
          class="colony-text-sm rounded border border-bark px-4 py-2 hover:bg-soil-deep"
          onClick={props.onCancel}
          disabled={working()}
        >
          Cancel
        </button>
        <Show when={!webFallback()}>
          <button
            class="colony-text-sm rounded border border-bark bg-soil-deep px-4 py-2 text-text-primary hover:bg-soil-light"
            onClick={openPicker}
            disabled={working()}
          >
            {working() ? "Opening…" : "🎯 Open picker"}
          </button>
        </Show>
      </div>
    </div>
  );
};
