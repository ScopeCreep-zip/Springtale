import type { Component, JSX } from "solid-js";
import { useI18n } from "./i18n/context";

export interface LayoutProps {
  children: JSX.Element;
  nav?: JSX.Element;
}

/**
 * Main layout — sidebar navigation + content area.
 *
 * Semantic HTML: <nav> with aria-label, <main> with skip-link target.
 * RTL: border-e (logical property) instead of border-r.
 */
export const Layout: Component<LayoutProps> = (props) => {
  const { t } = useI18n();

  return (
    <div class="flex min-h-screen bg-gray-950 text-gray-100">
      <a
        href="#main-content"
        class="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-accent focus:px-4 focus:py-2 focus:text-white"
      >
        {t("a11y.skipToContent")}
      </a>
      <nav aria-label={t("nav.label")} class="w-56 shrink-0 border-e border-gray-800 p-4">
        {props.nav}
      </nav>
      <main id="main-content" class="flex-1 p-6">{props.children}</main>
    </div>
  );
};
