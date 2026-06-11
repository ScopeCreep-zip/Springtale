/**
 * Typed IPC wrappers for template operations.
 */

import type { Template, WriteReport } from "@springtale/types";
import { invoke } from "@tauri-apps/api/core";

export async function listTemplates(): Promise<Template[]> {
  return invoke<Template[]>("list_templates");
}

export async function writeTemplate(name: string): Promise<WriteReport> {
  return invoke<WriteReport>("write_template", { name });
}
