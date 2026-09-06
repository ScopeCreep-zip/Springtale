import type { Session } from "@springtale/types";
import { apiGet, useI18n } from "@springtale/ui";
import { createSignal, For, onMount } from "solid-js";

/**
 * Sessions page — view active bot conversations.
 *
 * Shows per-user, per-channel conversation state. Useful for
 * monitoring which automated conversations are in progress
 * across all connectors.
 */
export function SessionsPage() {
  const { t } = useI18n();
  const [sessions, setSessions] = createSignal<Session[]>([]);
  const [error, setError] = createSignal("");
  const [loading, setLoading] = createSignal(true);

  const fetchSessions = async () => {
    try {
      setLoading(true);
      const data = await apiGet<{ sessions: Session[] }>("/sessions");
      setSessions(data.sessions ?? []);
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  onMount(fetchSessions);

  return (
    <div>
      <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-text-primary">{t("sessions.title")}</h1>
        <button
          type="button"
          class="rounded bg-soil-light px-3 py-1 text-sm text-text-primary hover:bg-bark"
          onClick={fetchSessions}
        >
          {t("common.refresh")}
        </button>
      </div>
      {error() && (
        <div
          role="alert"
          aria-live="assertive"
          class="mt-4 rounded border border-status-error/30 bg-status-error/10 p-3 text-sm text-status-error"
        >
          {error()}
        </div>
      )}
      {loading() ? (
        <p role="status" aria-live="polite" class="mt-4 text-text-dim">
          {t("common.loading")}
        </p>
      ) : (
        <ul class="mt-4 space-y-2">
          <For each={sessions()}>
            {(session) => (
              <li class="rounded border border-bark bg-soil-deep px-4 py-3">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-3">
                    <span class="font-medium text-text-primary">{session.user_id}</span>
                    <span class="text-text-dim">{t("sessions.in")}</span>
                    <span class="text-status-ok">{session.channel_id}</span>
                  </div>
                  <span class="text-xs text-text-dim">
                    {new Date(session.updated_at).toLocaleString()}
                  </span>
                </div>
                {session.pending_command && (
                  <p class="mt-1 text-sm text-status-warn">
                    {t("sessions.waiting", { cmd: session.pending_command })}
                  </p>
                )}
                {session.last_bot_message && (
                  <p class="mt-1 text-sm text-text-dim">
                    {t("sessions.last", { msg: session.last_bot_message })}
                  </p>
                )}
              </li>
            )}
          </For>
          {sessions().length === 0 && (
            <li class="list-none">
              <p role="status" class="text-text-dim">
                {t("empty.sessions")}
              </p>
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
