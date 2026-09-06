import { useI18n } from "@springtale/ui";
import { createSignal, Show } from "solid-js";
import { createVault, unlockVault, type VaultSession } from "./ipc/vault";

/**
 * Vault gate — the only screen that renders while the vault is locked.
 *
 * §2.1: unlocking is what starts the `springtaled` sidecar, so nothing
 * behind this screen can exist yet — there is no daemon to read, and no
 * stale colony data is left on screen after an auto-lock.
 */
export function VaultOverlay(props: { onUnlocked: (session: VaultSession) => void }) {
  const { t } = useI18n();
  const [passphrase, setPassphrase] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const open = async (fn: (p: string) => Promise<VaultSession>) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    try {
      const session = await fn(passphrase());
      setPassphrase("");
      props.onUnlocked(session);
    } catch (e) {
      // `create_vault` says "vault already exists — unlock it instead"
      // and `unlock_vault` "failed to open vault (wrong passphrase?)",
      // so the message itself tells first-run from wrong-passphrase.
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="flex h-screen w-full items-center justify-center bg-soil-deep p-6 text-text-primary">
      <div class="colony-modal w-full max-w-lg space-y-5 overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
        <h2 class="colony-text-md font-bold text-text-primary">{t("vault.title")}</h2>
        <p class="colony-text-xs text-text-dim">{t("vault.createDesc")}</p>
        <Show when={error()}>
          <div class="colony-text-2xs border border-status-error bg-status-error/10 p-2 text-status-error">
            {error()}
          </div>
        </Show>
        <div>
          <label for="vault-pass" class="colony-text-2xs text-text-secondary">
            {t("vault.passphrase")}
          </label>
          <input
            id="vault-pass"
            type="password"
            value={passphrase()}
            onInput={(e) => setPassphrase(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void open(unlockVault);
            }}
            class="colony-text-xs mt-2 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
          />
        </div>
        <div class="flex gap-3">
          <button
            type="button"
            disabled={busy()}
            onClick={() => void open(unlockVault)}
            class="colony-text-2xs border-2 border-status-ok bg-soil-light px-4 py-2 text-status-ok hover:bg-soil-deep disabled:opacity-50"
          >
            {t("vault.unlock")}
          </button>
          <button
            type="button"
            disabled={busy()}
            onClick={() => void open(createVault)}
            class="colony-text-2xs border-2 border-bark bg-soil-light px-4 py-2 text-text-secondary hover:bg-soil-deep disabled:opacity-50"
          >
            {t("vault.create")}
          </button>
        </div>
      </div>
    </div>
  );
}
