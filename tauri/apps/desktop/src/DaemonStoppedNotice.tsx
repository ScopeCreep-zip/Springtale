import { useI18n } from "@springtale/ui";
import { Show } from "solid-js";

/**
 * Shown when the `springtaled` sidecar stops on its own.
 *
 * The daemon owns the store, the scheduler and the bot loop, so once it
 * is gone the colony behind this screen is a still photograph of state
 * that no longer exists. Rendering it as if it were live would be the
 * fake signal the product model forbids — this replaces it, says what
 * happened, and offers the one action that actually recovers: lock, then
 * unlock, which spawns a fresh daemon.
 */
export function DaemonStoppedNotice(props: { code: number | null; onLock: () => void }) {
  const { t } = useI18n();

  return (
    <div class="flex h-screen w-full items-center justify-center bg-soil-deep p-6 text-text-primary">
      <div class="colony-modal w-full max-w-lg space-y-5 rounded border-2 border-status-error bg-soil-mid p-6">
        <h2 class="colony-text-md font-bold text-status-error">{t("daemon.stopped.title")}</h2>
        <p class="colony-text-xs text-text-secondary">{t("daemon.stopped.body")}</p>
        <Show when={props.code !== null}>
          <p class="colony-text-2xs border border-status-error bg-status-error/10 p-2 text-status-error">
            {t("daemon.stopped.code", { code: String(props.code) })}
          </p>
        </Show>
        <button
          type="button"
          onClick={() => props.onLock()}
          class="colony-text-xs w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary hover:border-accent"
        >
          {t("daemon.stopped.action")}
        </button>
      </div>
    </div>
  );
}
