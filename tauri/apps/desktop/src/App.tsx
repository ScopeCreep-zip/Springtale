import { createSignal, onMount, onCleanup, Show } from "solid-js";
import {
  ColonyShell,
  TopBar,
  Viewport,
  BottomPanel,
  HatchWizard,
  useDashboard,
  useI18n,
  seeded,
} from "@springtale/ui";
import type { ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection } from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { getVaultStatus, unlockVault, createVault } from "./ipc/vault";
import { panicWipe } from "./ipc/panic";

/**
 * Springtale Desktop — colony ecosystem dashboard.
 *
 * Trees=connectors, springtails=agents, mycelium=pipelines.
 * Pixel-art visualization with StarCraft-style bottom panel.
 */
export const App = () => {
  const db = useDashboard();
  const { t } = useI18n();

  // ── Desktop-only: vault state ──────────────────────────
  const [vaultLocked, setVaultLocked] = createSignal(true);
  const [showVault, setShowVault] = createSignal(false);
  const [passphrase, setPassphrase] = createSignal("");
  const [vaultError, setVaultError] = createSignal("");

  // ── Colony selection state ─────────────────────────────
  const [selection, setSelection] = createSignal<ColonySelection>({ id: null, type: null });

  // ── Auto-lock (Rust backend) ───────────────────────────
  const resetTimer = () => { invoke("reset_auto_lock").catch(() => {}); };

  onMount(async () => {
    document.addEventListener("mousemove", resetTimer);
    document.addEventListener("keydown", resetTimer);
    document.addEventListener("click", resetTimer);
    resetTimer();

    await listen("vault-locked", () => {
      setVaultLocked(true);
      setShowVault(true);
    });

    try {
      const v = await getVaultStatus();
      setVaultLocked(!v.unlocked);
      if (!v.unlocked) setShowVault(true);
    } catch (e) {
      const msg = String(e);
      if (msg.includes("vault file not found")) {
        setShowVault(true);
        setVaultLocked(true);
      }
    }

    await db.refresh();
  });

  onCleanup(() => {
    document.removeEventListener("mousemove", resetTimer);
    document.removeEventListener("keydown", resetTimer);
    document.removeEventListener("click", resetTimer);
  });

  // ── Vault handlers ─────────────────────────────────────
  const handleUnlock = async () => {
    try {
      const result = await unlockVault(passphrase());
      setVaultLocked(!result.unlocked);
      if (result.unlocked) setShowVault(false);
      setPassphrase("");
      setVaultError("");
      await db.refresh();
    } catch (e) {
      setVaultError(String(e));
    }
  };

  const handleCreateVault = async () => {
    try {
      const result = await createVault(passphrase());
      setVaultLocked(!result.unlocked);
      if (result.unlocked) setShowVault(false);
      setPassphrase("");
      setVaultError("");
      await db.refresh();
    } catch (e) {
      setVaultError(String(e));
    }
  };

  // ── Data → Colony visual model ─────────────────────────
  const TREE_TYPES = ["conifer", "deciduous", "shrub"] as const;

  const trees = (): ColonyTree[] =>
    db.connectors().map((c, i) => ({
      id: c.name,
      label: c.name,
      type: TREE_TYPES[seeded(c.name + "type", 0, 3)],
      x: seeded(c.name + "x", 8, 92),
      y: seeded(c.name + "y", 15, 70),
      status: c.enabled ? "active" as const : "idle" as const,
    }));

  const agents = (): ColonyAgent[] =>
    db.rules().map((r, i) => ({
      id: r.id,
      name: r.name,
      role: (["scout", "worker", "guard", "analyst"] as const)[seeded(r.id + "role", 0, 4)],
      autonomy: seeded(r.id + "auto", 0, 5),
      fuel: 50 + seeded(r.id + "fuel", 0, 50),
      hp: 80 + seeded(r.id + "hp", 0, 20),
      treeId: r.connector ?? r.triggerType,
      task: r.status === "enabled" ? `Processing ${r.triggerType}` : "Idle — disabled",
      status: r.status === "enabled" ? "ok" as const : "idle" as const,
      pipeline: r.triggerType,
    }));

  const connections = (): ColonyConnection[] => {
    const t = trees();
    if (t.length < 2) return [];
    const conns: ColonyConnection[] = [];
    for (let i = 0; i < t.length - 1 && i < 7; i++) {
      conns.push({
        a: t[i].id,
        b: t[(i + 1) % t.length].id,
        pipes: [{ id: `pipe-${t[i].id}-${t[(i + 1) % t.length].id}`, dir: 1, status: "active" }],
      });
    }
    return conns;
  };

  const formations = (): ColonyFormation[] =>
    db.swarms().map((s) => ({
      id: s.id,
      name: s.name,
      intent: s.intent.toUpperCase(),
      description: s.intent,
      momentum: s.status === "active" ? 2 : s.status === "paused" ? 1 : 0,
      momentumLabel: s.status === "active" ? "HOT" : s.status === "paused" ? "WARM" : "COLD",
      color: s.status === "active" ? "var(--color-momentum-hot)" : "var(--color-momentum-warm)",
      members: [],
      zone: { x: seeded(s.id + "zx", 20, 80), y: seeded(s.id + "zy", 20, 60) },
    }));

  // ── Command handler ────────────────────────────────────
  const handleCommand = (label: string) => {
    if (label === "SETTINGS") {
      setShowVault(true);
    }
  };

  // ── Vault overlay ──────────────────────────────────────
  const vaultOverlay = () => {
    if (!showVault()) return undefined;
    return (
      <div class="mx-auto max-w-md space-y-4 rounded border-2 border-bark bg-soil-mid p-6">
        <h2 class="font-bold text-text-primary" style={{ "font-size": "9px" }}>{t("vault.title")}</h2>
        <p class="text-text-dim" style={{ "font-size": "7px" }}>{vaultLocked() ? t("vault.createDesc") : ""}</p>
        {vaultError() && <div class="border border-status-error bg-status-error/10 p-2 text-status-error" style={{ "font-size": "6px" }}>{vaultError()}</div>}
        <div>
          <label for="vault-pass" class="text-text-secondary" style={{ "font-size": "6px" }}>{t("vault.passphrase")}</label>
          <input
            id="vault-pass"
            type="password"
            value={passphrase()}
            onInput={(e) => setPassphrase(e.currentTarget.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleUnlock(); }}
            class="mt-1 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary focus:border-accent focus:outline-none"
            style={{ "font-size": "7px" }}
          />
        </div>
        <div class="flex gap-2">
          <button onClick={handleUnlock} class="border-2 border-status-ok bg-soil-light px-3 py-1 text-status-ok hover:bg-soil-deep" style={{ "font-size": "6px" }}>
            {t("vault.unlock")}
          </button>
          <button onClick={handleCreateVault} class="border-2 border-bark bg-soil-light px-3 py-1 text-text-secondary hover:bg-soil-deep" style={{ "font-size": "6px" }}>
            {t("vault.create")}
          </button>
          <Show when={!vaultLocked()}>
            <button onClick={() => setShowVault(false)} class="px-3 py-1 text-text-dim hover:text-text-secondary" style={{ "font-size": "6px" }}>
              ✕
            </button>
          </Show>
        </div>
      </div>
    );
  };

  // ── Render ─────────────────────────────────────────────
  return (
    <ColonyShell
      topBar={
        <TopBar
          agents={agents()}
          trees={trees()}
          formations={formations()}
          selection={selection()}
          onSelectAgent={(id) => setSelection({ id, type: "agent" })}
          onSelectFormation={(id) => setSelection({ id, type: "formation" })}
        />
      }
      viewport={
        <Viewport
          trees={trees()}
          agents={agents()}
          connections={connections()}
          formations={formations()}
          events={db.events()}
          selection={selection()}
          onSelectTree={(id) => setSelection({ id, type: "tree" })}
          onSelectAgent={(id) => setSelection({ id, type: "agent" })}
          onSelectFormation={(id) => setSelection({ id, type: "formation" })}
          onClearSelection={() => setSelection({ id: null, type: null })}
          overlay={vaultOverlay()}
        />
      }
      bottomPanel={
        <BottomPanel
          trees={trees()}
          agents={agents()}
          connections={connections()}
          formations={formations()}
          selection={selection()}
          onCommand={handleCommand}
        />
      }
    />
  );
};
