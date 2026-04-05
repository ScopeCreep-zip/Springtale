/**
 * Travel mode IPC wrappers — encrypted backup + restore.
 *
 * Per ARCHITECTURE.md §2.6: "Exports encrypted backup to trusted
 * location. Wipes local data. On arrival: restore from backup."
 *
 * Passphrase crosses IPC once, used for KDF, then dropped.
 */
import { invoke } from "@tauri-apps/api/core";

export async function travelPrepare(
  passphrase: string,
  backupPath: string,
): Promise<void> {
  return invoke("travel_prepare", {
    passphrase,
    backupPath,
  });
}

export async function travelRestore(
  passphrase: string,
  backupPath: string,
): Promise<void> {
  return invoke("travel_restore", {
    passphrase,
    backupPath,
  });
}
