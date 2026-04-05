/**
 * Canvas/A2UI types — mirrors springtale_core::canvas::types.
 *
 * Keep in sync with Rust types. Canvas receives structured data,
 * never raw HTML (per ARCHITECTURE.md security requirements).
 */

export type StatusState = "Info" | "Success" | "Warning" | "Error" | "Loading";

export type CanvasBlock =
  | { type: "Text"; id: string; content: string }
  | { type: "Table"; id: string; headers: string[]; rows: string[][] }
  | { type: "KeyValue"; id: string; pairs: [string, string][] }
  | { type: "Status"; id: string; label: string; state: StatusState; message?: string };

export interface CanvasState {
  blocks: CanvasBlock[];
  title?: string;
  updated_at: string;
}

export type CanvasUpdate =
  | { action: "SetBlocks"; blocks: CanvasBlock[] }
  | { action: "UpdateBlock"; id: string; block: CanvasBlock }
  | { action: "RemoveBlock"; id: string }
  | { action: "Clear" };
