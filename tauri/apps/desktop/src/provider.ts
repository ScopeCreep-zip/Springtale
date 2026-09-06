/**
 * Desktop data provider — the web provider pointed at the sidecar.
 *
 * §2.1: `springtaled` is the only state owner. The desktop shell spawns
 * it as a Tauri sidecar at vault unlock, which hands back the loopback
 * port it bound and the derived API bearer token. Everything the UI
 * reads or writes then goes over the same HTTP + SSE API the web
 * dashboard uses — one provider implementation, no second copy of the
 * bot loop, no IPC mirror to keep in sync.
 */

import { configureApi, createWebProvider, type DataProvider } from "@springtale/ui";
import { openSelectorPicker } from "./ipc/selector_picker";

export function createDesktopProvider(port: number, token: string): DataProvider {
  configureApi(`http://127.0.0.1:${port}`, token);
  return {
    ...createWebProvider(),
    // The only method with no HTTP equivalent: picking a CSS selector
    // needs a real webview to overlay the target site, which a hosted
    // dashboard cannot do. Stays a Tauri command.
    openSelectorPicker,
  };
}
