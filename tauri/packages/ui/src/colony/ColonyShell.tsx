import type { Component, JSX } from "solid-js";
import { useI18n } from "../i18n/context";

export interface ColonyShellProps {
  topBar: JSX.Element;
  viewport: JSX.Element;
  bottomPanel: JSX.Element;
}

/**
 * Colony Shell — top-level layout matching colony-v8.html `.shell`.
 *
 * 3-row CSS grid: top bar (32px) + viewport (1fr) + bottom panel (148px).
 * Everything on one screen. No scrolling. No tabs.
 */
export const ColonyShell: Component<ColonyShellProps> = (props) => {
  const { t } = useI18n();

  return (
    <div class="grid h-screen grid-rows-[32px_1fr_148px] overflow-hidden bg-soil-deep text-text-primary">
      <a
        href="#colony-viewport"
        class="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-accent focus:px-4 focus:py-2 focus:text-white"
      >
        {t("a11y.skipToContent")}
      </a>

      {/* Top Bar — cadence + roster + summary */}
      <header class="flex items-center gap-1.5 border-b-2 border-bark bg-soil-mid px-2">
        {props.topBar}
      </header>

      {/* Viewport — the ecosystem canvas */}
      <main id="colony-viewport" class="relative overflow-hidden bg-soil-deep">
        {props.viewport}
      </main>

      {/* Bottom Panel — minimap + detail + commands */}
      <div class="grid grid-cols-[140px_1fr_170px] border-t-2 border-bark bg-soil-mid">
        {props.bottomPanel}
      </div>
    </div>
  );
};
