import { createSignal, onMount, For } from "solid-js";
import { useI18n } from "@springtale/ui";
import type { Session } from "@springtale/types";
import { get } from "../api/client";

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
      const data = await get<{ sessions: Session[] }>("/sessions");
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
        <h1 class="text-2xl font-bold text-white">{t("sessions.title")}</h1>
        <button
          class="rounded bg-gray-700 px-3 py-1 text-sm text-gray-200 hover:bg-gray-600"
          onClick={fetchSessions}
        >
          {t("common.refresh")}
        </button>
      </div>
      {error() && (
        <div role="alert" aria-live="assertive" class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
          {error()}
        </div>
      )}
      {loading() ? (
        <p role="status" aria-live="polite" class="mt-4 text-gray-400">{t("common.loading")}</p>
      ) : (
        <ul class="mt-4 space-y-2">
          <For each={sessions()}>
            {(session) => (
              <li class="rounded border border-gray-800 bg-gray-900 px-4 py-3">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-3">
                    <span class="font-medium text-white">{session.user_id}</span>
                    <span class="text-gray-500">{t("sessions.in")}</span>
                    <span class="text-blue-400">{session.channel_id}</span>
                  </div>
                  <span class="text-xs text-gray-500">
                    {new Date(session.updated_at).toLocaleString()}
                  </span>
                </div>
                {session.pending_command && (
                  <p class="mt-1 text-sm text-yellow-400">
                    {t("sessions.waiting", { cmd: session.pending_command })}
                  </p>
                )}
                {session.last_bot_message && (
                  <p class="mt-1 text-sm text-gray-400">
                    {t("sessions.last", { msg: session.last_bot_message })}
                  </p>
                )}
              </li>
            )}
          </For>
          {sessions().length === 0 && (
            <li class="list-none">
              <p role="status" class="text-gray-500">{t("empty.sessions")}</p>
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
