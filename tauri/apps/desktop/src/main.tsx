/* @refresh reload */
import { createRoot } from "solid-js";
import { render } from "solid-js/web";
import "./index.css";
import { createDashboardState, createI18n, DashboardProvider, I18nProvider } from "@springtale/ui";
import { App } from "./App";
import { createDesktopProvider } from "./provider";
import { registerQuickHide } from "./safety/quickhide";
import { installTrustedTypesPolicy } from "./safety/trusted-types";

// Trusted Types default policy — must run before SolidJS renders so the
// CSP `require-trusted-types-for 'script'` directive doesn't block legitimate
// reactive output. See ./safety/trusted-types.ts.
installTrustedTypesPolicy();

const i18n = createI18n("en");
const provider = createDesktopProvider();
// `createDashboardState` builds long-lived reactive computations
// (createResource etc.). Owning them under a `createRoot` keeps them
// disposable instead of leaking — and silences SolidJS's "computations
// created outside a createRoot or render will never be disposed" warning.
// This root lives for the whole app, so the dispose handle is intentionally
// unused.
const dashboard = createRoot(() => createDashboardState(provider));

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
