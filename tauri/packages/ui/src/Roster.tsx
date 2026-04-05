import { For, Show, createSignal } from "solid-js";
import type { Component, JSX } from "solid-js";
import { useI18n } from "./i18n/context";

export interface RuleItem {
  id: string;
  name: string;
  status: string;
  triggerType: string;
  connector?: string;
}

export interface RosterProps {
  rules: RuleItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewRule: () => void;
  safetyControls?: JSX.Element;
  systemStats?: JSX.Element;
}

/**
 * Roster — left sidebar showing rules grouped by connector + safety controls.
 *
 * RTS analogy: unit roster panel. Each rule is a "unit" with status.
 * Rules are grouped by their trigger connector for visual organization.
 */
export const Roster: Component<RosterProps> = (props) => {
  const { t } = useI18n();

  // Group rules by connector
  const grouped = () => {
    const groups: Record<string, RuleItem[]> = {};
    for (const rule of props.rules) {
      const key = rule.connector ?? "system";
      if (!groups[key]) groups[key] = [];
      groups[key].push(rule);
    }
    return groups;
  };

  return (
    <div class="flex h-full flex-col border-e border-gray-800 bg-gray-950">
      <div class="flex-1 overflow-y-auto p-3">
        <div class="flex items-center justify-between">
          <h2 class="text-xs font-semibold uppercase tracking-wider text-gray-500">
            {t("rules.title")}
          </h2>
          <button
            onClick={() => props.onNewRule()}
            class="rounded bg-gray-800 px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
          >
            +
          </button>
        </div>

        <div class="mt-3 space-y-3">
          <For each={Object.entries(grouped())}>
            {([connector, rules]) => (
              <div>
                <p class="text-xs font-medium text-gray-500">{connector}</p>
                <ul class="mt-1 space-y-1">
                  <For each={rules}>
                    {(rule) => (
                      <li>
                        <button
                          onClick={() => props.onSelect(rule.id)}
                          aria-current={props.selectedId === rule.id ? "true" : undefined}
                          class={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-start text-xs ${
                            props.selectedId === rule.id
                              ? "bg-gray-800 text-white"
                              : "text-gray-400 hover:bg-gray-800/50 hover:text-gray-200"
                          }`}
                        >
                          <span
                            class={`inline-block h-1.5 w-1.5 rounded-full ${
                              rule.status === "enabled" ? "bg-green-500" : "bg-gray-600"
                            }`}
                            aria-hidden="true"
                          />
                          <span class="truncate">{rule.name}</span>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </div>
            )}
          </For>
          <Show when={props.rules.length === 0}>
            <p class="text-xs text-gray-600">{t("empty.rules")}</p>
          </Show>
        </div>
      </div>

      <Show when={props.systemStats}>
        <div class="border-t border-gray-800 p-3 text-xs text-gray-500">
          {props.systemStats}
        </div>
      </Show>

      <Show when={props.safetyControls}>
        <div class="border-t border-gray-800 p-3">
          {props.safetyControls}
        </div>
      </Show>
    </div>
  );
};
