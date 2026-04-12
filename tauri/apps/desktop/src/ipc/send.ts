/**
 * Typed IPC wrappers for cross-channel send operations.
 */
import { invoke } from "@tauri-apps/api/core";
import type { SendRequest, SendOutcome } from "@springtale/types";

export async function sendMessage(req: SendRequest): Promise<SendOutcome> {
  return invoke<SendOutcome>("send_message", { req });
}
