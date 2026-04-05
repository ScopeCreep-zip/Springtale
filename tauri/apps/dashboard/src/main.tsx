/* @refresh reload */
import { render } from "solid-js/web";
import "./index.css";
import { App } from "./App";
import { createI18n, I18nProvider, createDashboardState, DashboardProvider } from "@springtale/ui";
import { createWebProvider } from "./provider";

const i18n = createI18n("en");
const provider = createWebProvider();
const dashboard = createDashboardState(provider);

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
