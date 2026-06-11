import type { ConnectorSchema } from "@springtale/types";
import type { ConditionDef } from "@springtale/ui";
import { ActionPicker, ConditionEditor, RulePreview, TriggerPicker, useI18n } from "@springtale/ui";
import { createSignal, onMount } from "solid-js";
import { getConnectorSchemas } from "../ipc/connectors";
import { createConnectorRule, getRuleSchema } from "../ipc/rules";

/**
 * Rule Builder page for Tauri desktop — same UI as dashboard,
 * but fetches data via IPC instead of HTTP.
 */
export function RuleBuilderPage() {
  const { t } = useI18n();
  const [connectors, setConnectors] = createSignal<ConnectorSchema[]>([]);
  const [name, setName] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [triggerConnector, setTriggerConnector] = createSignal("");
  const [triggerName, setTriggerName] = createSignal("");
  const [actionConnector, setActionConnector] = createSignal("");
  const [actionName, setActionName] = createSignal("");
  const [conditions, setConditions] = createSignal<ConditionDef[]>([]);
  const [conditionTypes, setConditionTypes] = createSignal<string[]>([]);
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal(false);

  onMount(async () => {
    try {
      setConnectors(await getConnectorSchemas());
      const schema = await getRuleSchema();
      const condObj = (schema as Record<string, unknown>).conditions as
        | Record<string, unknown>
        | undefined;
      if (condObj) {
        setConditionTypes(Object.keys(condObj));
      }
    } catch (e) {
      setError(String(e));
    }
  });

  const generateToml = (): string => {
    if (!name() || !triggerConnector() || !triggerName()) return "";
    const conditionsToml = conditions()
      .map((c) => {
        const entries = Object.entries(c)
          .filter(([k]) => k !== "type")
          .filter(([, v]) => v !== undefined && v !== "")
          .map(([k, v]) => `${k} = ${JSON.stringify(v)}`)
          .join(", ");
        return `{ type = "${c.type}", ${entries} }`;
      })
      .filter(Boolean)
      .join(",\n  ");

    return `[rule]
name = "${name()}"
description = "${description()}"
status = "disabled"

[trigger]
type = "ConnectorEvent"
connector = "${triggerConnector()}"
event = "${triggerName()}"

conditions = [
  ${conditionsToml}
]

[[actions]]
type = "RunConnector"
connector = "${actionConnector()}"
action = "${actionName()}"
params = {}
`;
  };

  const saveRule = async () => {
    try {
      await createConnectorRule({
        name: name(),
        trigger_connector: triggerConnector(),
        trigger_event: triggerName(),
        action_connector: actionConnector(),
        action_name: actionName(),
        conditions: conditions(),
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("builder.title")}</h1>
      <p class="mt-1 text-sm text-gray-400">{t("builder.description")}</p>
      {error() && (
        <div
          role="alert"
          aria-live="assertive"
          class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400"
        >
          {error()}
        </div>
      )}
      <div class="mt-6 grid grid-cols-2 gap-6">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            saveRule();
          }}
          class="space-y-6"
        >
          <div class="space-y-3">
            <div>
              <label for="dt-rule-name" class="block text-sm font-medium text-gray-300">
                {t("builder.ruleName")}
              </label>
              <input
                id="dt-rule-name"
                type="text"
                value={name()}
                onInput={(e) => setName(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
                placeholder={t("builder.ruleNamePlaceholder")}
              />
            </div>
            <div>
              <label for="dt-rule-desc" class="block text-sm font-medium text-gray-300">
                {t("builder.ruleDescription")}
              </label>
              <input
                id="dt-rule-desc"
                type="text"
                value={description()}
                onInput={(e) => setDescription(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
                placeholder={t("builder.ruleDescriptionPlaceholder")}
              />
            </div>
          </div>
          <div>
            <h3 class="text-sm font-semibold text-gray-200">{t("builder.whenTrigger")}</h3>
            <div class="mt-2">
              <TriggerPicker
                connectors={connectors()}
                onSelect={(c, tr) => {
                  setTriggerConnector(c);
                  setTriggerName(tr);
                }}
              />
            </div>
          </div>
          <ConditionEditor
            conditions={conditions()}
            conditionTypes={conditionTypes()}
            onChange={setConditions}
          />
          <div>
            <h3 class="text-sm font-semibold text-gray-200">{t("builder.thenAction")}</h3>
            <div class="mt-2">
              <ActionPicker
                connectors={connectors()}
                onSelect={(c, a) => {
                  setActionConnector(c);
                  setActionName(a);
                }}
              />
            </div>
          </div>
          <button
            type="submit"
            class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
            disabled={!name() || !triggerName() || !actionName()}
          >
            {saved() ? t("common.saved") : t("builder.saveRule")}
          </button>
        </form>
        <div>
          <RulePreview toml={generateToml()} />
        </div>
      </div>
    </div>
  );
}
