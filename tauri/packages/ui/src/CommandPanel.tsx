import { For, Show } from "solid-js";
import type { Component, JSX } from "solid-js";
import { useI18n } from "./i18n/context";

export interface RuleDetail {
  id: string;
  name: string;
  status: string;
  triggerType: string;
  triggerConfig: string;
  conditions: string[];
  actions: string[];
}

export interface EventItem {
  id: string;
  connectorName: string;
  triggerType: string;
  timestamp: string;
  actionTaken: string;
}

export interface CommandPanelProps {
  rule: RuleDetail | null;
  events: EventItem[];
  onToggle?: (id: string, enabled: boolean) => void;
  onDelete?: (id: string) => void;
  onNewRule?: () => void;
  ruleBuilder?: JSX.Element;
}

/**
 * Command Panel — shows selected rule details + recent events.
 *
 * RTS analogy: the orders panel. Shows what the selected unit can do,
 * its current configuration, and recent activity.
 */
export const CommandPanel: Component<CommandPanelProps> = (props) => {
  const { t } = useI18n();

  return (
    <div class="flex h-full flex-col border-t border-gray-800 bg-gray-900/50">
      <Show when={props.rule} fallback={
        <div class="flex flex-1 flex-col items-center justify-center p-6">
          <Show when={props.ruleBuilder} fallback={
            <div class="text-center">
              <p class="text-sm text-gray-500">{t("empty.rules")}</p>
              <button
                onClick={() => props.onNewRule?.()}
                class="mt-3 rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
              >
                + {t("condition.add")}
              </button>
            </div>
          }>
            {props.ruleBuilder}
          </Show>
        </div>
      }>
        {(rule) => (
          <div class="flex flex-1 flex-col overflow-hidden">
            <div class="flex items-center justify-between border-b border-gray-800 px-4 py-2">
              <div class="flex items-center gap-2">
                <span
                  class={`inline-block h-2 w-2 rounded-full ${
                    rule().status === "enabled" ? "bg-green-500" : "bg-gray-600"
                  }`}
                  aria-hidden="true"
                />
                <h3 class="text-sm font-semibold text-white">{rule().name}</h3>
                <span class="text-xs text-gray-500">{rule().triggerType}</span>
              </div>
              <div class="flex gap-2">
                <button
                  onClick={() => props.onToggle?.(rule().id, rule().status === "enabled")}
                  class="rounded px-2 py-1 text-xs text-gray-400 hover:bg-gray-800"
                >
                  {rule().status === "enabled" ? t("common.disable") : t("common.enable")}
                </button>
                <button
                  onClick={() => props.onDelete?.(rule().id)}
                  class="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-900/30"
                >
                  {t("common.delete")}
                </button>
              </div>
            </div>

            <div class="flex flex-1 overflow-hidden">
              <div class="flex-1 overflow-y-auto p-4">
                <div class="space-y-3 text-xs">
                  <div>
                    <span class="font-medium text-gray-400">Trigger:</span>
                    <span class="ms-2 text-gray-300">{rule().triggerConfig}</span>
                  </div>
                  <Show when={rule().conditions.length > 0}>
                    <div>
                      <span class="font-medium text-gray-400">Conditions:</span>
                      <ul class="mt-1 space-y-0.5 ps-4">
                        <For each={rule().conditions}>
                          {(c) => <li class="text-gray-300">{c}</li>}
                        </For>
                      </ul>
                    </div>
                  </Show>
                  <div>
                    <span class="font-medium text-gray-400">Actions:</span>
                    <ul class="mt-1 space-y-0.5 ps-4">
                      <For each={rule().actions}>
                        {(a) => <li class="text-gray-300">{a}</li>}
                      </For>
                    </ul>
                  </div>
                </div>
              </div>

              <div class="w-64 border-s border-gray-800 overflow-y-auto p-3">
                <h4 class="text-xs font-semibold uppercase tracking-wider text-gray-500">
                  {t("events.title")}
                </h4>
                <ul class="mt-2 space-y-1">
                  <For each={props.events}>
                    {(event) => (
                      <li class="text-xs">
                        <span class="text-gray-600">
                          {new Date(event.timestamp).toLocaleTimeString()}
                        </span>
                        <span class="ms-1 text-gray-400">{event.actionTaken}</span>
                      </li>
                    )}
                  </For>
                  <Show when={props.events.length === 0}>
                    <li class="text-xs text-gray-600">{t("empty.events")}</li>
                  </Show>
                </ul>
              </div>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
};
