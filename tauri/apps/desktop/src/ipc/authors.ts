/**
 * Typed IPC wrappers for trusted author management.
 */
import { invoke } from "@tauri-apps/api/core";

export async function listAuthors(): Promise<Array<{ name: string; pubkey: string }>> {
  const result = await invoke<{ authors: Array<{ name: string; pubkey: string }> }>("list_authors");
  return result.authors ?? [];
}

export async function addAuthor(name: string, pubkey: string): Promise<void> {
  return invoke("add_author", { name, pubkey });
}

export async function removeAuthor(name: string): Promise<void> {
  return invoke("remove_author", { name });
}
