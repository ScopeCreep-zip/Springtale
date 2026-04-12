/**
 * Canvas IPC wrappers — fetch state and listen for updates.
 *
 * Desktop uses Tauri events (push from Rust to frontend).
 * Dashboard uses SSE instead (same data, different transport).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CanvasState, CanvasUpdate } from "@springtale/types";

export interface ConnectionPipe {
  id: string;
  dir: 1 | -1;
  status: string;
}

export interface ConnectionData {
  a: string;
  b: string;
  pipes: ConnectionPipe[];
}

export async function getConnections(): Promise<ConnectionData[]> {
  return invoke<ConnectionData[]>("get_connections");
}

export async function getCanvasState(): Promise<CanvasState> {
  return invoke<CanvasState>("get_canvas_state");
}

export async function onCanvasUpdate(
  callback: (update: CanvasUpdate) => void,
): Promise<() => void> {
  return listen<CanvasUpdate>("canvas-update", (event) => {
    callback(event.payload);
  });
}
