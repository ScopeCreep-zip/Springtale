import { For, Show, createSignal } from "solid-js";
import type { Component } from "solid-js";
import type { ConnectorSchema } from "@springtale/types";
import { useI18n } from "./i18n/context";

export interface TriggerPickerProps {
  connectors: ConnectorSchema[];
  onSelect: (connector: string, trigger: string) => void;
}

/**
 * Trigger picker — select a trigger from installed connectors.
 *
 * Two-step dropdown: pick connector → pick trigger from that connector.
 * Labels linked to selects via id/for for screen reader accessibility.
 */
export const TriggerPicker: Component<TriggerPickerProps> = (props) => {
  const { t } = useI18n();
  const [selectedConnector, setSelectedConnector] = createSignal("");
  const [selectedTrigger, setSelectedTrigger] = createSignal("");

  const currentConnector = () =>
    props.connectors.find((c) => c.name === selectedConnector());

  const currentTrigger = () =>
    currentConnector()?.triggers.find((tr) => tr.name === selectedTrigger());

  const handleConnectorChange = (name: string) => {
    setSelectedConnector(name);
    setSelectedTrigger("");
  };

  const handleTriggerChange = (name: string) => {
    setSelectedTrigger(name);
    props.onSelect(selectedConnector(), name);
  };

  return (
    <div class="space-y-3">
      <div>
        <label for="trigger-connector" class="block colony-text-xs font-medium text-text-secondary">
          {t("picker.connector")}
        </label>
        <select
          id="trigger-connector"
          class="mt-1 w-full rounded border border-bark bg-soil-deep px-3 py-2 text-text-primary"
          value={selectedConnector()}
          onChange={(e) => handleConnectorChange(e.currentTarget.value)}
        >
          <option value="">{t("picker.selectConnector")}</option>
          <For each={props.connectors}>
            {(c) => <option value={c.name}>{c.name}</option>}
          </For>
        </select>
      </div>

      <Show when={currentConnector()}>
        <div>
          <label for="trigger-event" class="block colony-text-xs font-medium text-text-secondary">
            {t("picker.trigger")}
          </label>
          <select
            id="trigger-event"
            class="mt-1 w-full rounded border border-bark bg-soil-deep px-3 py-2 text-text-primary"
            value={selectedTrigger()}
            onChange={(e) => handleTriggerChange(e.currentTarget.value)}
          >
            <option value="">{t("picker.selectTrigger")}</option>
            <For each={currentConnector()?.triggers ?? []}>
              {(tr) => <option value={tr.name}>{tr.name}</option>}
            </For>
          </select>
        </div>
      </Show>

      <Show when={currentTrigger()}>
        <p class="colony-text-xs text-text-dim" aria-live="polite">
          {currentTrigger()?.description}
        </p>
      </Show>
    </div>
  );
};
