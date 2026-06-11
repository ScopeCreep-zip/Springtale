import { useI18n } from "@springtale/ui";
import { createSignal } from "solid-js";
import { travelPrepare, travelRestore } from "../ipc/travel";

/**
 * Travel mode page — encrypted backup + restore for border crossings.
 *
 * Per ARCHITECTURE.md §2.6:
 * - Prepare: export encrypted backup → wipe local data
 * - Restore: decrypt backup → restore vault + database + config
 *
 * The travel passphrase is separate from the vault passphrase.
 * It crosses IPC once, is used for Argon2id KDF, then dropped.
 */
export function TravelModePage() {
  const { t } = useI18n();

  // Prepare state
  const [prepPassphrase, setPrepPassphrase] = createSignal("");
  const [prepConfirm, setPrepConfirm] = createSignal("");
  const [prepPath, setPrepPath] = createSignal("");
  const [prepError, setPrepError] = createSignal("");
  const [preparing, setPreparing] = createSignal(false);

  // Restore state
  const [restorePassphrase, setRestorePassphrase] = createSignal("");
  const [restorePath, setRestorePath] = createSignal("");
  const [restoreError, setRestoreError] = createSignal("");
  const [restoring, setRestoring] = createSignal(false);
  const [restored, setRestored] = createSignal(false);

  const chooseSaveLocation = async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: "springtale-backup.enc",
      filters: [{ name: "Encrypted Backup", extensions: ["enc"] }],
    });
    if (path) setPrepPath(path);
  };

  const chooseBackupFile = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({
      filters: [{ name: "Encrypted Backup", extensions: ["enc"] }],
      multiple: false,
    });
    if (path) setRestorePath(path as string);
  };

  const handlePrepare = async () => {
    if (prepPassphrase() !== prepConfirm()) {
      setPrepError(t("travel.mismatch"));
      return;
    }
    if (!prepPassphrase() || !prepPath()) return;

    setPreparing(true);
    setPrepError("");
    try {
      await travelPrepare(prepPassphrase(), prepPath());
      // App will exit after wipe — this line may not execute
    } catch (e) {
      setPrepError(String(e));
      setPreparing(false);
    } finally {
      setPrepPassphrase("");
      setPrepConfirm("");
    }
  };

  const handleRestore = async () => {
    if (!restorePassphrase() || !restorePath()) return;

    setRestoring(true);
    setRestoreError("");
    try {
      await travelRestore(restorePassphrase(), restorePath());
      setRestored(true);
    } catch (e) {
      setRestoreError(String(e));
    } finally {
      setRestorePassphrase("");
      setRestoring(false);
    }
  };

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("travel.title")}</h1>

      <div class="mt-6 max-w-md space-y-8">
        <section aria-labelledby="prepare-heading">
          <h2 id="prepare-heading" class="text-lg font-semibold text-gray-200">
            {t("travel.prepareTitle")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">{t("travel.prepareDesc")}</p>

          {prepError() && (
            <div
              role="alert"
              aria-live="assertive"
              class="mt-3 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400"
            >
              {prepError()}
            </div>
          )}

          <form
            onSubmit={(e) => {
              e.preventDefault();
              handlePrepare();
            }}
            class="mt-3 space-y-3"
          >
            <div>
              <label for="prep-passphrase" class="block text-sm font-medium text-gray-300">
                {t("travel.passphrase")}
              </label>
              <input
                id="prep-passphrase"
                type="password"
                value={prepPassphrase()}
                onInput={(e) => setPrepPassphrase(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
              />
            </div>
            <div>
              <label for="prep-confirm" class="block text-sm font-medium text-gray-300">
                {t("travel.confirmPassphrase")}
              </label>
              <input
                id="prep-confirm"
                type="password"
                value={prepConfirm()}
                onInput={(e) => setPrepConfirm(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
              />
            </div>
            <div class="flex items-center gap-3">
              <button
                type="button"
                onClick={chooseSaveLocation}
                class="rounded bg-gray-700 px-4 py-2 text-sm font-medium text-gray-200 hover:bg-gray-600"
              >
                {t("travel.chooseLocation")}
              </button>
              {prepPath() && <span class="truncate text-sm text-gray-400">{prepPath()}</span>}
            </div>
            <button
              type="submit"
              disabled={!prepPassphrase() || !prepConfirm() || !prepPath() || preparing()}
              class="rounded bg-yellow-700 px-4 py-2 text-sm font-medium text-yellow-100 hover:bg-yellow-600 disabled:opacity-50"
            >
              {preparing() ? t("travel.preparing") : t("travel.prepare")}
            </button>
          </form>
        </section>

        <section aria-labelledby="restore-heading" class="border-t border-gray-800 pt-6">
          <h2 id="restore-heading" class="text-lg font-semibold text-gray-200">
            {t("travel.restoreTitle")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">{t("travel.restoreDesc")}</p>

          {restoreError() && (
            <div
              role="alert"
              aria-live="assertive"
              class="mt-3 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400"
            >
              {restoreError()}
            </div>
          )}

          {restored() ? (
            <div role="status" class="mt-3 rounded border border-green-500/30 bg-green-500/10 p-4">
              <p class="font-medium text-green-400">{t("travel.restoreSuccess")}</p>
            </div>
          ) : (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                handleRestore();
              }}
              class="mt-3 space-y-3"
            >
              <div class="flex items-center gap-3">
                <button
                  type="button"
                  onClick={chooseBackupFile}
                  class="rounded bg-gray-700 px-4 py-2 text-sm font-medium text-gray-200 hover:bg-gray-600"
                >
                  {t("travel.chooseBackup")}
                </button>
                {restorePath() && (
                  <span class="truncate text-sm text-gray-400">{restorePath()}</span>
                )}
              </div>
              <div>
                <label for="restore-passphrase" class="block text-sm font-medium text-gray-300">
                  {t("travel.passphrase")}
                </label>
                <input
                  id="restore-passphrase"
                  type="password"
                  value={restorePassphrase()}
                  onInput={(e) => setRestorePassphrase(e.currentTarget.value)}
                  class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
                />
              </div>
              <button
                type="submit"
                disabled={!restorePassphrase() || !restorePath() || restoring()}
                class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
              >
                {restoring() ? t("travel.restoring") : t("travel.restore")}
              </button>
            </form>
          )}
        </section>
      </div>
    </div>
  );
}
