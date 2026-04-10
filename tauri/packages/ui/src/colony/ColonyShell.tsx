import { Show } from "solid-js";
import type { Component, JSX } from "solid-js";
import { useI18n } from "../i18n/context";
import { useDashboard } from "../dashboard/context";

export interface ColonyShellProps {
  topBar: JSX.Element;
  viewport: JSX.Element;
  bottomPanel: JSX.Element;
  overlay?: JSX.Element;
  notification?: { message: string; type: "ok" | "warn" };
}

/**
 * Colony Shell — top-level layout matching colony-v8.html `.shell`.
 *
 * 3-row CSS grid: top bar (40px) + viewport (1fr) + bottom panel (180px).
 * Error banner shows between top bar and viewport when db.error() is set.
 * Full-screen overlay covers everything when shown (vault, hatch wizard, etc.).
 */
export const ColonyShell: Component<ColonyShellProps> = (props) => {
  const { t } = useI18n();
  const db = useDashboard();

  return (
    <div class="relative grid h-screen grid-rows-[40px_1fr_180px] overflow-hidden bg-soil-deep text-text-primary">
      <a
        href="#colony-viewport"
        class="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-accent focus:px-4 focus:py-2 focus:text-white"
      >
        {t("a11y.skipToContent")}
      </a>

      {/* Top Bar */}
      <header class="flex items-center gap-2 border-b-2 border-bark bg-soil-mid px-3">
        {props.topBar}
      </header>

      {/* Error banner — dismissible, shows db.error() */}
      <Show when={db.error()}>
        <div class="colony-text-2xs flex items-center justify-between border-b border-status-error bg-status-error/10 px-3 py-1 text-status-error">
          <span>{db.error()}</span>
          <button onClick={() => db.clearError()} class="colony-close-btn ml-2">✕</button>
        </div>
      </Show>

      {/* Notification banner — auto-dismiss, colored by type */}
      <Show when={props.notification}>
        <div class={`colony-text-2xs flex items-center px-3 py-1 ${
          props.notification!.type === "ok"
            ? "border-b border-status-ok bg-status-ok/10 text-status-ok"
            : "border-b border-status-warn bg-status-warn/10 text-status-warn"
        }`}>
          <span>{props.notification!.message}</span>
        </div>
      </Show>

      {/* Viewport */}
      <main id="colony-viewport" class="relative overflow-hidden bg-soil-deep">
        {props.viewport}
      </main>

      {/* Bottom Panel */}
      <div class="grid grid-cols-[160px_1fr_260px] border-t-2 border-bark bg-soil-mid">
        {props.bottomPanel}
      </div>

      {/* Full-screen overlay — covers EVERYTHING when shown */}
      <Show when={props.overlay}>
        <div class="absolute inset-0 z-50 flex items-center justify-center bg-soil-deep">
          {props.overlay}
        </div>
      </Show>
    </div>
  );
};
