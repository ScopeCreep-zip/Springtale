import { createSignal, For, Show } from "solid-js";
import type { Component } from "solid-js";
import type { ConnectorSchema } from "@springtale/types";
import { useI18n } from "./i18n/context";

export interface HatchWizardProps {
  connectors: ConnectorSchema[];
  onHatch: (name: string, intent: string, connectors: string[], triggerConnector: string, triggerEvent: string, actionConnector: string, actionName: string) => void;
  onCancel: () => void;
}

/**
 * Hatch Wizard — 4-step swarm creation flow.
 *
 * Like clicking a barracks in StarCraft → picking Marine → watching it train.
 * Steps:
 * 1. Substrate — pick connectors (where the swarm works)
 * 2. Decomposition — pick trigger + action (what it processes)
 * 3. Intent — name + behavior mode (how it operates)
 * 4. Hatch — review + deploy
 *
 * Springtail metaphor: hatching agents into a substrate to decompose
 * detritus into humus. The furcula (trigger) releases when conditions
 * are met, agents emit pheromones (signals), and the swarm self-organizes.
 */
export const HatchWizard: Component<HatchWizardProps> = (props) => {
  const { t } = useI18n();
  const [step, setStep] = createSignal(1);

  // Step 1: Substrate selection
  const [selectedConnectors, setSelectedConnectors] = createSignal<string[]>([]);

  // Step 2: Decomposition (trigger → action)
  const [triggerConnector, setTriggerConnector] = createSignal("");
  const [triggerEvent, setTriggerEvent] = createSignal("");
  const [actionConnector, setActionConnector] = createSignal("");
  const [actionName, setActionName] = createSignal("");

  // Step 3: Intent
  const [name, setName] = createSignal("");
  const [intent, setIntent] = createSignal("Reconnoiter");

  const toggleConnector = (c: string) => {
    setSelectedConnectors((prev) =>
      prev.includes(c) ? prev.filter((x) => x !== c) : [...prev, c],
    );
  };

  const selectedSchemas = () =>
    props.connectors.filter((c) => selectedConnectors().includes(c.name));

  const canProceedStep1 = () => selectedConnectors().length > 0;
  const canProceedStep2 = () => triggerConnector() && triggerEvent() && actionConnector() && actionName();
  const canProceedStep3 = () => name().trim().length > 0;

  const handleHatch = () => {
    props.onHatch(
      name(),
      intent(),
      selectedConnectors(),
      triggerConnector(),
      triggerEvent(),
      actionConnector(),
      actionName(),
    );
  };

  return (
    <div class="mx-auto max-w-lg space-y-4">
      {/* Step indicator */}
      <div class="flex items-center gap-2 text-xs text-gray-500">
        <For each={[1, 2, 3, 4]}>
          {(s) => (
            <div class={`flex items-center gap-1 ${step() >= s ? "text-green-400" : "text-gray-600"}`}>
              <span class={`inline-flex h-5 w-5 items-center justify-center rounded-full text-xs ${
                step() === s ? "bg-blue-600 text-white" : step() > s ? "bg-green-800 text-green-300" : "bg-gray-800 text-gray-500"
              }`}>{s}</span>
              <span class="hidden sm:inline">
                {s === 1 ? "Substrate" : s === 2 ? "Decomposition" : s === 3 ? "Intent" : "Hatch"}
              </span>
              <Show when={s < 4}>
                <span class="text-gray-700">→</span>
              </Show>
            </div>
          )}
        </For>
      </div>

      {/* Step 1: Substrate (pick connectors) */}
      <Show when={step() === 1}>
        <div class="space-y-3">
          <h3 class="text-sm font-semibold text-white">Choose Substrates</h3>
          <p class="text-xs text-gray-500">Where should this swarm operate? Select the platforms it will connect to.</p>
          <div class="grid grid-cols-2 gap-2">
            <For each={props.connectors}>
              {(c) => (
                <button
                  onClick={() => toggleConnector(c.name)}
                  class={`rounded border p-2 text-start text-xs ${
                    selectedConnectors().includes(c.name)
                      ? "border-green-500/50 bg-green-900/20 text-white"
                      : "border-gray-700 bg-gray-800/50 text-gray-400 hover:border-gray-600"
                  }`}
                >
                  <span class="font-medium">{c.name}</span>
                  <p class="mt-0.5 text-gray-500">
                    {c.triggers.length} triggers · {c.actions.length} actions
                  </p>
                </button>
              )}
            </For>
          </div>
          <Show when={props.connectors.length === 0}>
            <p class="text-xs text-gray-600">{t("empty.connectors")}</p>
          </Show>
          <div class="flex justify-between">
            <button onClick={props.onCancel} class="text-xs text-gray-500 hover:text-gray-300">Cancel</button>
            <button
              onClick={() => setStep(2)}
              disabled={!canProceedStep1()}
              class="rounded bg-blue-600 px-4 py-1.5 text-xs text-white hover:bg-blue-500 disabled:opacity-50"
            >
              Next →
            </button>
          </div>
        </div>
      </Show>

      {/* Step 2: Decomposition (trigger → action) */}
      <Show when={step() === 2}>
        <div class="space-y-3">
          <h3 class="text-sm font-semibold text-white">Define Decomposition</h3>
          <p class="text-xs text-gray-500">What triggers this swarm (furcula), and what does it produce (humus)?</p>

          <div>
            <label for="hw-trigger-connector" class="block text-xs font-medium text-gray-400">Trigger substrate</label>
            <select
              id="hw-trigger-connector"
              value={triggerConnector()}
              onChange={(e) => { setTriggerConnector(e.currentTarget.value); setTriggerEvent(""); }}
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-white"
            >
              <option value="">Select...</option>
              <For each={selectedSchemas()}>
                {(c) => <option value={c.name}>{c.name}</option>}
              </For>
            </select>
          </div>

          <Show when={triggerConnector()}>
            <div>
              <label for="hw-trigger-event" class="block text-xs font-medium text-gray-400">Trigger event (furcula)</label>
              <select
                id="hw-trigger-event"
                value={triggerEvent()}
                onChange={(e) => setTriggerEvent(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-white"
              >
                <option value="">Select...</option>
                <For each={selectedSchemas().find((c) => c.name === triggerConnector())?.triggers ?? []}>
                  {(tr) => <option value={tr.name}>{tr.name}</option>}
                </For>
              </select>
            </div>
          </Show>

          <div>
            <label for="hw-action-connector" class="block text-xs font-medium text-gray-400">Action substrate</label>
            <select
              id="hw-action-connector"
              value={actionConnector()}
              onChange={(e) => { setActionConnector(e.currentTarget.value); setActionName(""); }}
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-white"
            >
              <option value="">Select...</option>
              <For each={selectedSchemas()}>
                {(c) => <option value={c.name}>{c.name}</option>}
              </For>
            </select>
          </div>

          <Show when={actionConnector()}>
            <div>
              <label for="hw-action-name" class="block text-xs font-medium text-gray-400">Action (humus output)</label>
              <select
                id="hw-action-name"
                value={actionName()}
                onChange={(e) => setActionName(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-white"
              >
                <option value="">Select...</option>
                <For each={selectedSchemas().find((c) => c.name === actionConnector())?.actions ?? []}>
                  {(a) => <option value={a.name}>{a.name}</option>}
                </For>
              </select>
            </div>
          </Show>

          <div class="flex justify-between">
            <button onClick={() => setStep(1)} class="text-xs text-gray-500 hover:text-gray-300">← Back</button>
            <button
              onClick={() => setStep(3)}
              disabled={!canProceedStep2()}
              class="rounded bg-blue-600 px-4 py-1.5 text-xs text-white hover:bg-blue-500 disabled:opacity-50"
            >
              Next →
            </button>
          </div>
        </div>
      </Show>

      {/* Step 3: Intent */}
      <Show when={step() === 3}>
        <div class="space-y-3">
          <h3 class="text-sm font-semibold text-white">Set Intent</h3>
          <p class="text-xs text-gray-500">Name your swarm and set its behavioral intent.</p>

          <div>
            <label for="hw-name" class="block text-xs font-medium text-gray-400">Swarm name</label>
            <input
              id="hw-name"
              type="text"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              placeholder="e.g., GitHub Issue Monitor"
              class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
            />
          </div>

          <fieldset>
            <legend class="text-xs font-medium text-gray-400">Intent pattern</legend>
            <div class="mt-1 space-y-1">
              <For each={[
                { value: "Reconnoiter", label: "Reconnoiter", desc: "Gather info, monitor, read-only" },
                { value: "Execute", label: "Execute", desc: "Take action on targets" },
                { value: "Stabilize", label: "Stabilize", desc: "Maintain state, defensive" },
                { value: "Surge", label: "Surge", desc: "Maximum effort on one objective" },
              ]}>
                {(opt) => (
                  <label class={`flex cursor-pointer items-center gap-2 rounded border p-2 text-xs ${
                    intent() === opt.value ? "border-blue-500/50 bg-blue-900/20" : "border-gray-800 hover:border-gray-700"
                  }`}>
                    <input
                      type="radio"
                      name="intent"
                      value={opt.value}
                      checked={intent() === opt.value}
                      onChange={() => setIntent(opt.value)}
                      class="h-3 w-3"
                    />
                    <div>
                      <span class="font-medium text-white">{opt.label}</span>
                      <span class="ms-2 text-gray-500">{opt.desc}</span>
                    </div>
                  </label>
                )}
              </For>
            </div>
          </fieldset>

          <div class="flex justify-between">
            <button onClick={() => setStep(2)} class="text-xs text-gray-500 hover:text-gray-300">← Back</button>
            <button
              onClick={() => setStep(4)}
              disabled={!canProceedStep3()}
              class="rounded bg-blue-600 px-4 py-1.5 text-xs text-white hover:bg-blue-500 disabled:opacity-50"
            >
              Next →
            </button>
          </div>
        </div>
      </Show>

      {/* Step 4: Hatch (review + deploy) */}
      <Show when={step() === 4}>
        <div class="space-y-3">
          <h3 class="text-sm font-semibold text-white">Ready to Hatch</h3>

          <div class="rounded border border-gray-700 bg-gray-800/50 p-3 text-xs space-y-2">
            <div class="flex justify-between">
              <span class="text-gray-400">Swarm</span>
              <span class="text-white">{name()}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">Intent</span>
              <span class="text-white">{intent()}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">Substrates</span>
              <span class="text-white">{selectedConnectors().join(", ")}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">Trigger</span>
              <span class="text-white">{triggerConnector()} → {triggerEvent()}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">Action</span>
              <span class="text-white">{actionConnector()} → {actionName()}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">Agents</span>
              <span class="text-white">{selectedConnectors().length}</span>
            </div>
          </div>

          <div class="flex justify-between">
            <button onClick={() => setStep(3)} class="text-xs text-gray-500 hover:text-gray-300">← Back</button>
            <button
              onClick={handleHatch}
              class="rounded bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-500"
            >
              🌱 Hatch Swarm
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};
