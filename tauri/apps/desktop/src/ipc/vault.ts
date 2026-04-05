/**
 * Typed IPC wrappers for vault operations.
 *
 * The vault passphrase NEVER leaves Rust — it goes directly from
 * the IPC call to the Vault::open() method. The frontend only
 * receives a status response (unlocked, duress session).
 */
import { invoke } from "@tauri-apps/api/core";

export interface VaultStatus {
  unlocked: boolean;
  duress_session: boolean;
}

export async function createVault(passphrase: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("create_vault", { passphrase });
}

export async function unlockVault(passphrase: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("unlock_vault", { passphrase });
}

export async function lockVault(): Promise<void> {
  return invoke("lock_vault");
}

export async function getVaultStatus(): Promise<VaultStatus> {
  return invoke<VaultStatus>("get_vault_status");
}
