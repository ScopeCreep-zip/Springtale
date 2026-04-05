/**
 * Desktop data provider — wraps Tauri IPC modules into DataProvider interface.
 *
 * Thin adapter: no logic, just maps existing ipc/ functions to the
 * platform-agnostic DataProvider that createDashboardState() expects.
 */
import type { DataProvider } from "@springtale/ui";
import { listen } from "@tauri-apps/api/event";
import type { EventEntry, CanvasUpdate } from "@springtale/types";

import { listConnectors, getConnectorSchemas } from "./ipc/connectors";
import { listRules, createRule, toggleRule, deleteRule } from "./ipc/rules";
import { listEvents } from "./ipc/events";
import {
  listFormations,
  createFormation,
  deployFormation,
  pauseFormation,
  resumeFormation,
  dissolveFormation,
} from "./ipc/formations";
import { getCanvasState } from "./ipc/canvas";

export function createDesktopProvider(): DataProvider {
  return {
    listConnectors,
    getConnectorSchemas,
    listRules,
    createRule,
    toggleRule,
    deleteRule,
    listEvents,

    subscribeToEvents(callback) {
      let unlisten: (() => void) | undefined;
      listen<EventEntry>("event-fired", (e) => callback(e.payload))
        .then((u) => { unlisten = u; })
        .catch(() => {});
      return () => unlisten?.();
    },

    listFormations,
    createFormation,
    deployFormation,
    pauseFormation,
    resumeFormation,
    dissolveFormation,
    getCanvasState,

    subscribeToCanvasUpdates(callback) {
      let unlisten: (() => void) | undefined;
      listen<CanvasUpdate>("canvas-update", (e) => callback(e.payload))
        .then((u) => { unlisten = u; })
        .catch(() => {});
      return () => unlisten?.();
    },
  };
}
