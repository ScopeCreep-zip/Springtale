import type { Component, JSX } from "solid-js";
import { useI18n } from "./i18n/context";

export interface DashboardProps {
  /** Resource bar (top) — connector status, vault, event count */
  resourceBar: JSX.Element;
  /** Roster (left sidebar) — rule list, safety controls */
  roster: JSX.Element;
  /** Main canvas (center) — primary content area */
  canvas: JSX.Element;
  /** Command panel (bottom) — selected rule details + events */
  commandPanel: JSX.Element;
}

/**
 * Dashboard — unified single-surface layout.
 *
 * RTS-inspired: resource bar (top), unit roster (left),
 * main canvas (center), command panel (bottom).
 * Everything visible on one screen. No tab-flipping.
 *
 * For activists: see all your automations, connectors,
 * and safety controls at a glance.
 */
export const Dashboard: Component<DashboardProps> = (props) => {
  const { t } = useI18n();

  return (
    <div class="flex h-screen flex-col bg-gray-950 text-gray-100">
      <a
        href="#main-canvas"
        class="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-accent focus:px-4 focus:py-2 focus:text-white"
      >
        {t("a11y.skipToContent")}
      </a>

      {/* Resource Bar (top) */}
      <header>{props.resourceBar}</header>

      {/* Main area: Roster + Canvas */}
      <div class="flex flex-1 overflow-hidden">
        {/* Roster (left sidebar) */}
        <aside class="w-48 shrink-0 overflow-hidden">
          {props.roster}
        </aside>

        {/* Canvas + Command Panel (right area) */}
        <main id="main-canvas" class="flex flex-1 flex-col overflow-hidden">
          {/* Canvas (main workspace) */}
          <div class="flex-1 overflow-y-auto p-4">
            {props.canvas}
          </div>

          {/* Command Panel (bottom) */}
          <div class="h-64 shrink-0 overflow-hidden">
            {props.commandPanel}
          </div>
        </main>
      </div>
    </div>
  );
};
