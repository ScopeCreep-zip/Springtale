/**
 * Typed IPC wrappers for cross-channel send operations.
 */

import type { SendOutcome, SendRequest } from "@springtale/types";
import { invoke } from "@tauri-apps/api/core";

export async function sendMessage(req: SendRequest): Promise<SendOutcome> {
  return invoke<SendOutcome>("send_message", { req });
}
