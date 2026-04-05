import { createSignal, onMount, For } from "solid-js";
import { StatusBadge, useI18n } from "@springtale/ui";
import {
  listConnectors,
  enableConnector,
  disableConnector,
  type ConnectorInfo,
} from "../ipc/connectors";

export function ConnectorsPage() {
  const { t } = useI18n();
  const [connectors, setConnectors] = createSignal<ConnectorInfo[]>([]);
  const [error, setError] = createSignal("");

  const fetch = async () => {
    try {
      setConnectors(await listConnectors());
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  const toggle = async (name: string, enabled: boolean) => {
    try {
      if (enabled) {
        await disableConnector(name);
      } else {
        await enableConnector(name);
      }
      await fetch();
    } catch (e) {
      setError(String(e));
    }
  };

  onMount(fetch);

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("connectors.title")}</h1>
      {error() && (
        <div role="alert" aria-live="assertive" class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
          {error()}
        </div>
      )}
      <ul class="mt-4 space-y-2">
        <For each={connectors()}>
          {(c) => (
            <li class="flex items-center justify-between rounded border border-gray-800 bg-gray-900 px-4 py-3">
              <div class="flex items-center gap-2">
                <span class="font-medium text-white">{c.name}</span>
                <StatusBadge status={c.enabled ? "enabled" : "disabled"} />
              </div>
              <button
                class="rounded bg-gray-700 px-3 py-1 text-sm text-gray-200 hover:bg-gray-600"
                onClick={() => toggle(c.name, c.enabled)}
              >
                {c.enabled ? t("common.disable") : t("common.enable")}
              </button>
            </li>
          )}
        </For>
        {connectors().length === 0 && (
          <li class="list-none">
            <p role="status" class="text-gray-500">{t("empty.connectors")}</p>
          </li>
        )}
      </ul>
    </div>
  );
}
