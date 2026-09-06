/**
 * Safety operations that return no JSON body — panic wipe and travel
 * mode (§2.6).
 *
 * These used to be desktop Tauri commands against an in-process
 * runtime. The daemon owns the vault, database and config now, so both
 * shells drive them over the same HTTP API
 * (`apps/springtaled/src/api/safety.rs` / `travel.rs`, which wrap
 * `springtale_runtime::operations::{safety,travel}`).
 *
 * The paths are local: `backup_path` is resolved by the daemon, which
 * runs on the same machine as the UI (desktop sidecar) or is the
 * machine the user is administering (web dashboard).
 */
import { getBaseUrl, getToken } from "./client";

/** POST with an empty/ignored response body. Throws the server's text. */
async function postVoid(path: string, body: unknown): Promise<void> {
  const response = await fetch(`${getBaseUrl()}${path}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${getToken()}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const detail = (await response.text().catch(() => "")).trim();
    throw new Error(detail || `API error: ${response.status} ${response.statusText}`);
  }
}

/**
 * Emergency data destruction (§2.6 — must complete within 3 seconds).
 *
 * The daemon wipes vault, database and config and then exits, so the
 * connection usually dies before a response arrives. `fetch` reports
 * that as a `TypeError`, which here is the shape of success — only a
 * real HTTP error (which carries a status and a body) is surfaced.
 */
export async function panicWipe(): Promise<void> {
  try {
    await postVoid("/safety/panic-wipe", {});
  } catch (e) {
    if (e instanceof TypeError) return;
    throw e;
  }
}

/**
 * Travel prepare — encrypted backup to `backupPath`, then wipe local
 * data. The travel passphrase is separate from the vault passphrase;
 * it crosses the wire once (loopback, bearer-authenticated), is used
 * for the Argon2id KDF, and is never stored.
 */
export async function travelPrepare(passphrase: string, backupPath: string): Promise<void> {
  await postVoid("/travel/prepare", { passphrase, backup_path: backupPath });
}

/** Travel restore — decrypt `backupPath` back into vault + db + config. */
export async function travelRestore(passphrase: string, backupPath: string): Promise<void> {
  await postVoid("/travel/restore", { passphrase, backup_path: backupPath });
}
