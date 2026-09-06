/* @refresh reload */
import { createRoot } from "solid-js";
import { render } from "solid-js/web";
import "./index.css";
import {
  createDashboardState,
  createI18n,
  createWebProvider,
  DashboardProvider,
  I18nProvider,
} from "@springtale/ui";
import { App } from "./App";
import { installTrustedTypesPolicy } from "./safety/trusted-types";

// Trusted Types default policy — kill-switch under
// `Content-Security-Policy: require-trusted-types-for 'script'`.
// See ./safety/trusted-types.ts.
installTrustedTypesPolicy();

const i18n = createI18n("en");
const provider = createWebProvider();
// Own the dashboard's long-lived reactive computations under a root so they
// don't leak (and so SolidJS doesn't warn about computations created outside
// a createRoot/render). Lives for the whole app; dispose handle unused.
const dashboard = createRoot(() => createDashboardState(provider));

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

// W5 PWA — register the app-shell service worker so the dashboard is
// installable on mobile. Caches static assets only; never API/SSE traffic
// (see public/sw.js). Best-effort: a failure just means no offline shell.
if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js").catch(() => {
      // No offline shell — the app still works online.
    });
  });
}
