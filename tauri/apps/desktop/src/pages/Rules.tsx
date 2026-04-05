import { createSignal, onMount, For } from "solid-js";
import { StatusBadge, useI18n } from "@springtale/ui";
import { listRules, toggleRule, deleteRule, type RuleSummary } from "../ipc/rules";

export function RulesPage() {
  const { t } = useI18n();
  const [rules, setRules] = createSignal<RuleSummary[]>([]);
  const [error, setError] = createSignal("");

  const fetch = async () => {
    try {
      setRules(await listRules());
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  const toggle = async (id: string, status: string) => {
    try {
      await toggleRule(id, status !== "enabled");
      await fetch();
    } catch (e) {
      setError(String(e));
    }
  };

  const remove = async (id: string) => {
    try {
      await deleteRule(id);
      await fetch();
    } catch (e) {
      setError(String(e));
    }
  };

  onMount(fetch);

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("rules.title")}</h1>
      {error() && (
        <div role="alert" aria-live="assertive" class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
          {error()}
        </div>
      )}
      <ul class="mt-4 space-y-2">
        <For each={rules()}>
          {(rule) => (
            <li class="flex items-center justify-between rounded border border-gray-800 bg-gray-900 px-4 py-3">
              <div class="space-y-1">
                <div class="flex items-center gap-2">
                  <span class="font-medium text-white">{rule.name}</span>
                  <StatusBadge
                    status={rule.status === "enabled" ? "enabled" : "disabled"}
                    label={rule.status}
                  />
                </div>
                <p class="text-xs text-gray-500">
                  {t("rules.trigger", { type: rule.trigger_type })}
                </p>
              </div>
              <div class="flex gap-2">
                <button
                  class="rounded bg-gray-700 px-3 py-1 text-sm text-gray-200 hover:bg-gray-600"
                  onClick={() => toggle(rule.id, rule.status)}
                >
                  {rule.status === "enabled" ? t("common.disable") : t("common.enable")}
                </button>
                <button
                  class="rounded bg-red-900/50 px-3 py-1 text-sm text-red-300 hover:bg-red-800/50"
                  onClick={() => remove(rule.id)}
                >
                  {t("common.delete")}
                </button>
              </div>
            </li>
          )}
        </For>
        {rules().length === 0 && (
          <li class="list-none">
            <p role="status" class="text-gray-500">{t("empty.rules")}</p>
          </li>
        )}
      </ul>
    </div>
  );
}
