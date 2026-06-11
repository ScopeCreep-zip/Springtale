import type { EventEntry } from "@springtale/types";
import { useI18n } from "@springtale/ui";
import { createSignal, For, onMount } from "solid-js";
import { listEvents } from "../ipc/events";

export function EventsPage() {
  const { t } = useI18n();
  const [events, setEvents] = createSignal<EventEntry[]>([]);
  const [error, setError] = createSignal("");

  const fetch = async () => {
    try {
      setEvents(await listEvents(50));
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  onMount(fetch);

  return (
    <div>
      <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-white">{t("events.title")}</h1>
        <button
          type="button"
          class="rounded bg-gray-700 px-3 py-1 text-sm text-gray-200 hover:bg-gray-600"
          onClick={fetch}
        >
          {t("common.refresh")}
        </button>
      </div>
      {error() && (
        <div
          role="alert"
          aria-live="assertive"
          class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400"
        >
          {error()}
        </div>
      )}
      <ul class="mt-4 space-y-1" aria-live="polite">
        <For each={events()}>
          {(event) => (
            <li class="flex items-center gap-3 rounded border border-gray-800 bg-gray-900 px-3 py-2 text-sm">
              <span class="shrink-0 text-gray-500">
                {new Date(event.timestamp).toLocaleTimeString()}
              </span>
              <span class="font-medium text-blue-400">{event.connector_name}</span>
              <span class="text-gray-300">{event.trigger_type}</span>
              <span class="text-gray-500">{event.action_taken}</span>
            </li>
          )}
        </For>
        {events().length === 0 && (
          <li class="list-none">
            <p role="status" class="text-gray-500">
              {t("empty.events")}
            </p>
          </li>
        )}
      </ul>
    </div>
  );
}
