import { useI18n } from "@springtale/ui";
import { createSignal, onMount } from "solid-js";
import {
  createVault,
  getVaultStatus,
  lockVault,
  unlockVault,
  type VaultStatus,
} from "../ipc/vault";

/**
 * Vault page — create, unlock, or lock the vault.
 *
 * First-time users see a vault creation wizard.
 * Returning users see the unlock form.
 * Passphrase goes directly to Rust via IPC — never stored
 * in frontend memory beyond the input field.
 */
export function VaultPage() {
  const { t } = useI18n();
  const [status, setStatus] = createSignal<VaultStatus | null>(null);
  const [passphrase, setPassphrase] = createSignal("");
  const [confirmPassphrase, setConfirmPassphrase] = createSignal("");
  const [error, setError] = createSignal("");
  const [vaultExists, setVaultExists] = createSignal(true);

  onMount(async () => {
    try {
      const s = await getVaultStatus();
      setStatus(s);
      setVaultExists(true);
    } catch (e) {
      const msg = String(e);
      if (msg.includes("vault file not found")) {
        setVaultExists(false);
      } else {
        setError(msg);
      }
    }
  });

  const create = async () => {
    if (passphrase() !== confirmPassphrase()) {
      setError(t("travel.mismatch"));
      return;
    }
    if (!passphrase()) return;

    try {
      const result = await createVault(passphrase());
      setStatus(result);
      setPassphrase("");
      setConfirmPassphrase("");
      setVaultExists(true);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  const unlock = async () => {
    try {
      const result = await unlockVault(passphrase());
      setStatus(result);
      setPassphrase("");
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  const lock = async () => {
    try {
      await lockVault();
      setStatus({ unlocked: false, duress_session: false });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("vault.title")}</h1>
      {error() && (
        <div
          role="alert"
          aria-live="assertive"
          class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400"
        >
          {error()}
        </div>
      )}

      {!vaultExists() ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            create();
          }}
          class="mt-6 max-w-md space-y-4"
        >
          <p class="text-sm text-gray-400">{t("vault.createDesc")}</p>
          <div>
            <label for="new-passphrase" class="block text-sm font-medium text-gray-300">
              {t("vault.passphrase")}
            </label>
            <input
              id="new-passphrase"
              type="password"
              value={passphrase()}
              onInput={(e) => setPassphrase(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
            />
          </div>
          <div>
            <label for="confirm-passphrase" class="block text-sm font-medium text-gray-300">
              {t("travel.confirmPassphrase")}
            </label>
            <input
              id="confirm-passphrase"
              type="password"
              value={confirmPassphrase()}
              onInput={(e) => setConfirmPassphrase(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
            />
          </div>
          <button
            type="submit"
            disabled={!passphrase() || !confirmPassphrase()}
            class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
          >
            {t("vault.create")}
          </button>
        </form>
      ) : status()?.unlocked ? (
        <div class="mt-6 space-y-4">
          <div role="status" class="rounded border border-green-500/30 bg-green-500/10 p-4">
            <p class="font-medium text-green-400">{t("vault.unlocked")}</p>
          </div>
          <button
            type="button"
            class="rounded bg-gray-700 px-4 py-2 text-sm font-medium text-gray-200 hover:bg-gray-600"
            onClick={lock}
          >
            {t("vault.lock")}
          </button>
        </div>
      ) : (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            unlock();
          }}
          class="mt-6 max-w-md space-y-4"
        >
          <div>
            <label for="vault-passphrase" class="block text-sm font-medium text-gray-300">
              {t("vault.passphrase")}
            </label>
            <input
              id="vault-passphrase"
              type="password"
              value={passphrase()}
              onInput={(e) => setPassphrase(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
              placeholder={t("vault.passphrasePlaceholder")}
            />
          </div>
          <button
            type="submit"
            class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
          >
            {t("vault.unlock")}
          </button>
        </form>
      )}
    </div>
  );
}
