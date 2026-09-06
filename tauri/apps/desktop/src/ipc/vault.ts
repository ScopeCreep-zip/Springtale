/**
 * Typed IPC wrappers for vault operations.
 *
 * The vault passphrase NEVER leaves Rust — it goes directly from
 * the IPC call to the Vault::open() method. The frontend only
 * receives a status response (unlocked, duress session).
 *
 * Unlocking also starts the `springtaled` sidecar and hands back the
 * loopback port it bound plus the API bearer token, which is everything
 * `createDesktopProvider` needs to talk to it. The token is one the
 * daemon ISSUED: Rust logs in to the sidecar with the passphrase once it
 * reports READY and passes on the random session token it gets back
 * (plan 6.6). Nothing is derived, and the passphrase still never crosses
 * IPC in the other direction.
 */
import { invoke } from "@tauri-apps/api/core";

export interface VaultStatus {
  unlocked: boolean;
  duress_session: boolean;
}

/** `create_vault` / `unlock_vault` response: status + sidecar handle. */
export interface VaultSession {
  status: VaultStatus;
  port: number;
  token: string;
}

/** First run — create the vault, which also starts the daemon on it. */
export async function createVault(passphrase: string): Promise<VaultSession> {
  return invoke<VaultSession>("create_vault", { passphrase });
}

export async function unlockVault(passphrase: string): Promise<VaultSession> {
  return invoke<VaultSession>("unlock_vault", { passphrase });
}

export async function lockVault(): Promise<void> {
  return invoke("lock_vault");
}

export async function getVaultStatus(): Promise<VaultStatus> {
  return invoke<VaultStatus>("get_vault_status");
}
