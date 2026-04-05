import { For, Show } from "solid-js";
import type { Component } from "solid-js";
import { useI18n } from "./i18n/context";

export interface ConnectorStatus {
  name: string;
  enabled: boolean;
}

export interface ResourceBarProps {
  connectors: ConnectorStatus[];
  vaultLocked: boolean;
  eventCount: number;
  onConnectorClick?: (name: string) => void;
  onVaultClick?: () => void;
}

/**
 * Resource Bar — top strip showing connector status, vault, event count.
 *
 * RTS analogy: resource indicators at the top of the screen.
 * Each connector is a "resource" the user's bots can deploy.
 */
export const ResourceBar: Component<ResourceBarProps> = (props) => {
  const { t } = useI18n();

  return (
    <div class="flex items-center justify-between border-b border-gray-800 bg-gray-950 px-4 py-2">
      <div class="flex items-center gap-3">
        <span class="text-sm font-semibold text-white">{t("app.title")}</span>
        <div class="flex items-center gap-2">
          <For each={props.connectors}>
            {(c) => (
              <button
                onClick={() => props.onConnectorClick?.(c.name)}
                class="flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
                aria-label={`${c.name}: ${c.enabled ? t("status.enabled") : t("status.disabled")}`}
              >
                <span
                  class={`inline-block h-2 w-2 rounded-full ${c.enabled ? "bg-green-500" : "bg-gray-600"}`}
                  aria-hidden="true"
                />
                {c.name}
              </button>
            )}
          </For>
          <Show when={props.connectors.length === 0}>
            <span class="text-xs text-gray-600">{t("empty.connectors")}</span>
          </Show>
        </div>
      </div>

      <div class="flex items-center gap-4 text-xs">
        <button
          onClick={() => props.onVaultClick?.()}
          class={`flex items-center gap-1 ${props.vaultLocked ? "text-red-400" : "text-green-400"}`}
        >
          {props.vaultLocked ? "🔒" : "🔓"}
          <span>{props.vaultLocked ? t("vault.lock") : t("vault.unlocked")}</span>
        </button>
        <span class="text-gray-500">
          ⚡ {props.eventCount}
        </span>
      </div>
    </div>
  );
};
