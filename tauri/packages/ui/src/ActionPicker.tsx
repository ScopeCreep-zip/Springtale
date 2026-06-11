import type { ConnectorSchema } from "@springtale/types";
import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { useI18n } from "./i18n/context";

export interface ActionPickerProps {
  connectors: ConnectorSchema[];
  onSelect: (connector: string, action: string) => void;
}

/**
 * Action picker — select an action from installed connectors.
 *
 * Same two-step pattern as TriggerPicker: connector → action.
 * Labels linked to selects via id/for for screen reader accessibility.
 */
export const ActionPicker: Component<ActionPickerProps> = (props) => {
  const { t } = useI18n();
  const [selectedConnector, setSelectedConnector] = createSignal("");
  const [selectedAction, setSelectedAction] = createSignal("");

  const currentConnector = () => props.connectors.find((c) => c.name === selectedConnector());

  const currentAction = () => currentConnector()?.actions.find((a) => a.name === selectedAction());

  const handleConnectorChange = (name: string) => {
    setSelectedConnector(name);
    setSelectedAction("");
  };

  const handleActionChange = (name: string) => {
    setSelectedAction(name);
    props.onSelect(selectedConnector(), name);
  };

  return (
    <div class="space-y-3">
      <div>
        <label for="action-connector" class="block colony-text-xs font-medium text-text-secondary">
          {t("picker.connector")}
        </label>
        <select
          id="action-connector"
          class="mt-1 w-full rounded border border-bark bg-soil-deep px-3 py-2 text-text-primary"
          value={selectedConnector()}
          onChange={(e) => handleConnectorChange(e.currentTarget.value)}
        >
          <option value="">{t("picker.selectConnector")}</option>
          <For each={props.connectors}>{(c) => <option value={c.name}>{c.name}</option>}</For>
        </select>
      </div>

      <Show when={currentConnector()}>
        <div>
          <label for="action-name" class="block colony-text-xs font-medium text-text-secondary">
            {t("picker.action")}
          </label>
          <select
            id="action-name"
            class="mt-1 w-full rounded border border-bark bg-soil-deep px-3 py-2 text-text-primary"
            value={selectedAction()}
            onChange={(e) => handleActionChange(e.currentTarget.value)}
          >
            <option value="">{t("picker.selectAction")}</option>
            <For each={currentConnector()?.actions ?? []}>
              {(a) => <option value={a.name}>{a.name}</option>}
            </For>
          </select>
        </div>
      </Show>

      <Show when={currentAction()}>
        <p class="colony-text-xs text-text-dim" aria-live="polite">
          {currentAction()?.description}
        </p>
      </Show>
    </div>
  );
};
