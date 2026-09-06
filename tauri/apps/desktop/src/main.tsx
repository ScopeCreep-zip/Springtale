/* @refresh reload */
import { render } from "solid-js/web";
import "./index.css";
import { createI18n, I18nProvider } from "@springtale/ui";
import { App } from "./App";
import { registerQuickHide } from "./safety/quickhide";
import { installTrustedTypesPolicy } from "./safety/trusted-types";

// Trusted Types default policy — must run before SolidJS renders so the
// CSP `require-trusted-types-for 'script'` directive doesn't block legitimate
// reactive output. See ./safety/trusted-types.ts.
installTrustedTypesPolicy();

const i18n = createI18n("en");

// §2.8: Register quick-hide global shortcut (Ctrl+Shift+H)
registerQuickHide().catch(() => {
  // Non-fatal: shortcut may fail if already registered or platform doesn't support
});

// The dashboard state is NOT built here: it needs a provider, and the
// provider needs the sidecar's port + token, which only exist after the
// vault is unlocked. `App` owns that gate.
const root = document.getElementById("root");
if (root) {
  render(
    () => (
      <I18nProvider value={i18n}>
        <App />
      </I18nProvider>
    ),
    root,
  );
}
