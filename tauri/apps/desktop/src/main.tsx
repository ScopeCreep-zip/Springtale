/* @refresh reload */
import { render } from "solid-js/web";
import "./index.css";
import { App } from "./App";
import { createI18n, I18nProvider, createDashboardState, DashboardProvider } from "@springtale/ui";
import { registerQuickHide } from "./safety/quickhide";
import { createDesktopProvider } from "./provider";

const i18n = createI18n("en");
const provider = createDesktopProvider();
const dashboard = createDashboardState(provider);

// §2.8: Register quick-hide global shortcut (Ctrl+Shift+H)
registerQuickHide().catch(() => {
  // Non-fatal: shortcut may fail if already registered or platform doesn't support
});

const root = document.getElementById("root");
if (root) {
  render(
    () => (
      <I18nProvider value={i18n}>
        <DashboardProvider value={dashboard}>
          <App />
        </DashboardProvider>
      </I18nProvider>
    ),
    root,
  );
}
