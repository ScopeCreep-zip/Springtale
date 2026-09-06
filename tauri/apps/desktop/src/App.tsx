import {
  closeAllStreams,
  createDashboardState,
  DashboardProvider,
  type DashboardState,
} from "@springtale/ui";
import { listen } from "@tauri-apps/api/event";
import { createSignal, onMount, Show } from "solid-js";
import { Colony } from "./Colony";
import { lockVault, type VaultSession } from "./ipc/vault";
import { createDesktopProvider } from "./provider";
import { VaultOverlay } from "./VaultOverlay";

/**
 * Desktop shell root — the vault gate.
 *
 * §2.1: the desktop is a *client* of `springtaled`. Unlocking the vault
 * is what spawns the sidecar, so the data provider cannot exist before
 * then — it is built from the port and token `unlock_vault` returns.
 * Until that happens there is nothing to render but the passphrase
 * screen, and when the vault locks again (auto-lock, quick-hide, or the
 * Settings button) the daemon is killed and the whole colony tree is
 * torn down with it.
 */
export const App = () => {
  const [dashboard, setDashboard] = createSignal<DashboardState | null>(null);

  const openSession = (session: VaultSession) => {
    const provider = createDesktopProvider(session.port, session.token);
    setDashboard(createDashboardState(provider));
  };

  const closeSession = () => {
    // The daemon is gone; drop its streams so the reconnect loops don't
    // chase a dead port, and drop the state so no colony data survives
    // behind the lock screen.
    closeAllStreams();
    setDashboard(null);
  };

  onMount(async () => {
    // Auto-lock timeout and `lock_vault` both land here.
    await listen("vault-locked", closeSession);

    // G5g — the OS-wide quick-hide hotkey. The Rust handler has already
    // hidden the window; mirror the in-window path by locking, which
    // emits "vault-locked" and tears the session down above.
    await listen("quick-hide", () => {
      lockVault().catch(() => {});
    });
  });

  return (
    <Show when={dashboard()} fallback={<VaultOverlay onUnlocked={openSession} />}>
      {(db) => (
        <DashboardProvider value={db()}>
          <Colony onLock={() => void lockVault()} />
        </DashboardProvider>
      )}
    </Show>
  );
};
