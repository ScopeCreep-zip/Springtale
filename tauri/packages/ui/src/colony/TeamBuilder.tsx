/**
 * TeamBuilder — OOBE team composition screen.
 *
 * Inspired by XCOM's pre-task squad loadout: single panel where you see
 * the whole team while configuring individual members. No screen-switching
 * (per strategy game UX best practices).
 *
 * Sections (all visible, scrollable):
 * 1. TASK — describe what you want to automate (NL input or skip)
 * 2. SERVICES — connector grid, click to configure
 * 3. SQUAD — agent slots (trigger → action), add/remove/configure
 * 4. TEAM OVERVIEW — intent, guard mode, AI config, deploy button
 *
 * References:
 * - XCOM squad loadout (slot-based, equipment per soldier)
 * - DRG class selection (freely swap before deploying)
 * - Zapier onboarding (3-minute first automation, NL copilot)
 * - Strategy game UI (minimize screen-switching, show micro+macro)
 */

import type { AvailableConnector, ConnectorSchema } from "@springtale/types";
import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";

/** A single agent slot in the squad. */
interface AgentSlot {
  id: number;
  connectorName: string;
  triggerName: string;
  actionConnector: string;
  actionName: string;
}

export interface TeamConfig {
  name: string;
  intent: string;
  agents: AgentSlot[];
  guardMode: boolean;
}

export interface TeamBuilderProps {
  availableConnectors: AvailableConnector[];
  connectors: ConnectorSchema[];
  intents: Array<{ value: string; label: string }>;
  onSetupConnector: (name: string) => void;
  onParseRule?: (intent: string) => Promise<Record<string, unknown>>;
  onSaveAiConfig?: (config: Record<string, unknown>) => Promise<void>;
  onDeploy: (team: TeamConfig) => Promise<void>;
  onCancel: () => void;
  /**
   * W2.A — optional pre-loaded recipe. When set, TeamBuilder
   * initialises team name, intent (from the recipe's category), and
   * pre-selects the recipe's connectors. The user can edit any field
   * before deploying or save the result as a fork (W2.B).
   *
   * The shape is intentionally narrow — TeamBuilder remains the
   * "compose-from-parts" surface and shouldn't grow recipe-specific
   * logic. The seed is just initial signal values, not a permanent
   * link back to the recipe.
   */
  initialTemplate?: TeamBuilderSeed;
}

/** Subset of a `Recipe` TeamBuilder actually uses on init. */
export interface TeamBuilderSeed {
  name: string;
  intent?: string;
  connectorsUsed: string[];
}

export const TeamBuilder: Component<TeamBuilderProps> = (props) => {
  // ── Task description ──
  const [taskIntent, setTaskIntent] = createSignal("");
  const [aiLoading, setAiLoading] = createSignal(false);
  const [_aiResult, setAiResult] = createSignal<Record<string, unknown> | null>(null);

  // ── Inline AI config (shown when AI not yet configured) ──
  const [showAiSetup, setShowAiSetup] = createSignal(false);
  const [aiAdapterType, setAiAdapterType] = createSignal("ollama");
  const [aiBaseUrl, setAiBaseUrl] = createSignal("");
  const [aiApiKey, setAiApiKey] = createSignal("");
  const [aiModel, setAiModel] = createSignal("");
  const [aiSaving, setAiSaving] = createSignal(false);

  // ── Selected services ──
  // W2.A — pre-seed from `initialTemplate.connectorsUsed` when set.
  const [selectedServices, setSelectedServices] = createSignal<Set<string>>(
    new Set<string>(props.initialTemplate?.connectorsUsed ?? []),
  );

  // ── Squad (agent slots) ──
  let nextSlotId = 1;
  const [agents, setAgents] = createSignal<AgentSlot[]>([]);
  const [editingSlot, setEditingSlot] = createSignal<number | null>(null);

  // ── Team config ──
  // W2.A — pre-seed name + intent from the optional template.
  const [teamName, setTeamName] = createSignal(props.initialTemplate?.name ?? "");
  const [intent, setIntent] = createSignal(
    props.initialTemplate?.intent ?? props.intents[0]?.value ?? "",
  );
  const [guardMode, setGuardMode] = createSignal(false);
  const [deploying, setDeploying] = createSignal(false);
  const [error, setError] = createSignal("");

  // ── Derived ──
  const _loadedServices = () =>
    props.availableConnectors.filter((a) => a.loaded && selectedServices().has(a.name));

  const allSchemas = () => {
    const loaded = new Map(props.connectors.map((c) => [c.name, c]));
    const result = [...props.connectors];
    for (const avail of props.availableConnectors) {
      if (!loaded.has(avail.name) && selectedServices().has(avail.name)) {
        result.push({
          name: avail.name,
          version: "",
          description: "",
          triggers: avail.triggers.map((t) => ({
            name: t.name,
            description: t.description,
            schema: t.schema,
          })),
          actions: avail.actions.map((a) => ({
            name: a.name,
            description: a.description,
            input_schema: a.input_schema,
            output_schema: a.output_schema,
          })),
        });
      }
    }
    return result;
  };

  const canDeploy = () => agents().length > 0 && teamName().trim().length > 0;

  // ── Handlers ──
  const toggleService = (name: string) => {
    setSelectedServices((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const applyAiResult = (rule: Record<string, unknown>) => {
    setAiResult(rule);
    const trigger = rule.trigger as Record<string, unknown> | undefined;
    const actions = rule.actions as Array<Record<string, unknown>> | undefined;
    if (trigger && actions && actions.length > 0) {
      const connName = (trigger as { connector?: string }).connector ?? "";
      const trigName = (trigger as { event?: string }).event ?? "";
      const firstAction = actions[0] as { connector?: string; action?: string } | undefined;
      const actConn = firstAction?.connector ?? "";
      const actName = firstAction?.action ?? "";

      if (connName) {
        setSelectedServices((prev) => {
          const next = new Set(prev);
          next.add(connName);
          return next;
        });
      }
      if (actConn) {
        setSelectedServices((prev) => {
          const next = new Set(prev);
          next.add(actConn);
          return next;
        });
      }

      setAgents([
        {
          id: nextSlotId++,
          connectorName: connName,
          triggerName: trigName,
          actionConnector: actConn,
          actionName: actName,
        },
      ]);

      if (!teamName()) {
        setTeamName((rule.name as string) ?? "My Team");
      }
    }
  };

  const handleAiGenerate = async () => {
    if (!props.onParseRule) return;
    // No intent text yet — just open AI setup so they can configure
    if (!taskIntent().trim()) {
      setShowAiSetup(true);
      return;
    }
    setAiLoading(true);
    setError("");
    try {
      const rule = await props.onParseRule(taskIntent());
      applyAiResult(rule);
    } catch (e) {
      const msg = String(e);
      if (
        msg.includes("disabled") ||
        msg.includes("NoopAdapter") ||
        msg.includes("not configured")
      ) {
        setShowAiSetup(true);
      } else {
        setError(msg);
      }
    } finally {
      setAiLoading(false);
    }
  };

  const handleSaveAiConfig = async () => {
    if (!props.onSaveAiConfig) return;
    setAiSaving(true);
    setError("");
    try {
      // Send raw fields — backend applies adapter-specific defaults
      const config: Record<string, unknown> = {
        type: aiAdapterType(),
        base_url: aiBaseUrl() || undefined,
        api_key: aiApiKey() || undefined,
        model: aiModel() || undefined,
      };
      await props.onSaveAiConfig(config);
      setShowAiSetup(false);
      // Now retry the AI generation with the newly configured adapter
      if (taskIntent().trim() && props.onParseRule) {
        setAiLoading(true);
        try {
          const rule = await props.onParseRule(taskIntent());
          applyAiResult(rule);
        } catch (e) {
          setError(String(e));
        } finally {
          setAiLoading(false);
        }
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setAiSaving(false);
    }
  };

  const addAgent = () => {
    setAgents((prev) => [
      ...prev,
      { id: nextSlotId++, connectorName: "", triggerName: "", actionConnector: "", actionName: "" },
    ]);
    setEditingSlot(nextSlotId - 1);
  };

  const removeAgent = (id: number) => {
    setAgents((prev) => prev.filter((a) => a.id !== id));
    if (editingSlot() === id) setEditingSlot(null);
  };

  const updateAgent = (id: number, field: keyof AgentSlot, value: string) => {
    setAgents((prev) => prev.map((a) => (a.id === id ? { ...a, [field]: value } : a)));
  };

  const handleDeploy = async () => {
    if (!canDeploy()) return;
    setDeploying(true);
    setError("");
    try {
      await props.onDeploy({
        name: teamName(),
        intent: intent(),
        agents: agents(),
        guardMode: guardMode(),
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setDeploying(false);
    }
  };

  const triggersForConnector = (connName: string) =>
    allSchemas().find((s) => s.name === connName)?.triggers ?? [];

  const actionsForConnector = (connName: string) =>
    allSchemas().find((s) => s.name === connName)?.actions ?? [];

  const selectedConnectorNames = () =>
    [...selectedServices()].filter((name) => {
      const avail = props.availableConnectors.find((a) => a.name === name);
      return avail?.loaded || !avail?.requires_config;
    });

  return (
    <div class="space-y-4">
      <div class="mb-2 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">Build Your Bot/Team</h2>
        <button type="button" onClick={props.onCancel} class="colony-close-btn">
          ✕
        </button>
      </div>

      {error() && (
        <div class="colony-text-2xs border border-status-error bg-status-error/10 p-2 text-status-error">
          {error()}
        </div>
      )}

      {/* ── TASK ── */}
      <section>
        <h3 class="colony-label mb-1">TASK</h3>
        <p class="colony-text-3xs mb-2 text-text-dim">What do you want to automate?</p>
        <div class="flex gap-2">
          <input
            type="text"
            value={taskIntent()}
            onInput={(e) => setTaskIntent(e.currentTarget.value)}
            placeholder="e.g., Monitor GitHub repos and post to Slack"
            class="colony-text-2xs flex-1 border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
          />
          <Show when={props.onParseRule}>
            <button
              type="button"
              onClick={handleAiGenerate}
              disabled={aiLoading()}
              class="colony-text-2xs border-2 border-accent bg-soil-light px-3 py-1.5 text-accent hover:bg-soil-deep disabled:opacity-50"
            >
              {aiLoading() ? "Generating..." : "AI"}
            </button>
          </Show>
        </div>
        <Show when={!props.onParseRule}>
          <p class="colony-text-3xs mt-1 text-text-dim">
            No AI configured — pick services and agents manually below
          </p>
        </Show>

        {/* Inline AI setup — appears when AI button is clicked but no adapter is configured */}
        <Show when={showAiSetup()}>
          <div class="mt-3 rounded border-2 border-accent/50 bg-soil-deep p-3 space-y-3">
            <div class="flex items-center justify-between">
              <p class="colony-text-2xs font-bold text-accent">Set up AI adapter</p>
              <button
                type="button"
                onClick={() => setShowAiSetup(false)}
                class="colony-text-3xs text-text-dim hover:text-text-secondary"
              >
                skip
              </button>
            </div>
            <div class="space-y-1.5">
              <For
                each={[
                  {
                    value: "ollama",
                    label: "Ollama (Local)",
                    desc: "Private — no data leaves your machine",
                  },
                  {
                    value: "openai",
                    label: "OpenAI Compatible",
                    desc: "GPT, Gemini, DeepSeek, OpenRouter",
                  },
                  { value: "anthropic", label: "Anthropic", desc: "Claude with native tool use" },
                ]}
              >
                {(opt) => (
                  <label
                    class={`flex cursor-pointer items-start gap-2 rounded border-2 p-2 ${
                      aiAdapterType() === opt.value
                        ? "border-status-ok bg-status-ok/5"
                        : "border-bark hover:border-bark-light"
                    }`}
                  >
                    <input
                      type="radio"
                      name="oobe-ai"
                      value={opt.value}
                      checked={aiAdapterType() === opt.value}
                      onChange={() => setAiAdapterType(opt.value)}
                      class="mt-0.5"
                    />
                    <div>
                      <span class="colony-text-2xs font-bold text-text-primary">{opt.label}</span>
                      <p class="colony-text-3xs text-text-dim">{opt.desc}</p>
                    </div>
                  </label>
                )}
              </For>
            </div>
            <Show when={aiAdapterType() === "ollama"}>
              <div class="space-y-2">
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">Ollama URL</span>
                  <input
                    type="text"
                    value={aiBaseUrl()}
                    onInput={(e) => setAiBaseUrl(e.currentTarget.value)}
                    placeholder="http://127.0.0.1:11434"
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">Model</span>
                  <input
                    type="text"
                    value={aiModel()}
                    onInput={(e) => setAiModel(e.currentTarget.value)}
                    placeholder="llama3.2"
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
              </div>
            </Show>
            <Show when={aiAdapterType() === "openai"}>
              <div class="space-y-2">
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">API Base URL</span>
                  <input
                    type="text"
                    value={aiBaseUrl()}
                    onInput={(e) => setAiBaseUrl(e.currentTarget.value)}
                    placeholder="https://api.openai.com"
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">API Key</span>
                  <input
                    type="password"
                    value={aiApiKey()}
                    onInput={(e) => setAiApiKey(e.currentTarget.value)}
                    placeholder="sk-..."
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">Model</span>
                  <input
                    type="text"
                    value={aiModel()}
                    onInput={(e) => setAiModel(e.currentTarget.value)}
                    placeholder="gpt-4o"
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
              </div>
            </Show>
            <Show when={aiAdapterType() === "anthropic"}>
              <div class="space-y-2">
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">API Key</span>
                  <input
                    type="password"
                    value={aiApiKey()}
                    onInput={(e) => setAiApiKey(e.currentTarget.value)}
                    placeholder="sk-ant-..."
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
                <label class="block">
                  <span class="colony-text-3xs text-text-secondary">Model</span>
                  <input
                    type="text"
                    value={aiModel()}
                    onInput={(e) => setAiModel(e.currentTarget.value)}
                    placeholder="claude-sonnet-4-20250514"
                    class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                  />
                </label>
              </div>
            </Show>
            <button
              type="button"
              onClick={handleSaveAiConfig}
              disabled={aiSaving()}
              class="colony-text-2xs border-2 border-status-ok bg-soil-light px-3 py-1.5 text-status-ok hover:bg-soil-deep disabled:opacity-50"
            >
              {aiSaving() ? "Saving..." : "Save & Generate"}
            </button>
          </div>
        </Show>
      </section>

      {/* ── SERVICES ── */}
      <section>
        <h3 class="colony-label mb-1">SERVICES</h3>
        <div class="grid grid-cols-3 gap-1.5">
          <For each={props.availableConnectors}>
            {(conn) => {
              const isSelected = () => selectedServices().has(conn.name);
              const isLoaded = () => conn.loaded || !conn.requires_config;
              return (
                <button
                  type="button"
                  onClick={() => {
                    if (!isLoaded() && !isSelected()) {
                      props.onSetupConnector(conn.name);
                    }
                    toggleService(conn.name);
                  }}
                  class={`rounded border-2 p-1.5 text-start ${
                    isSelected()
                      ? "border-status-ok bg-status-ok/10 text-text-primary"
                      : "border-bark bg-soil-deep text-text-secondary hover:border-bark-light"
                  }`}
                >
                  <span class="colony-text-2xs font-bold">
                    {conn.name.replace("connector-", "")}
                  </span>
                  <Show when={!isLoaded()}>
                    <span class="colony-text-3xs ml-1 text-status-warn">(configure)</span>
                  </Show>
                  <Show when={isLoaded() && isSelected()}>
                    <span class="colony-text-3xs ml-1 text-status-ok">✓</span>
                  </Show>
                  <p class="colony-text-3xs text-text-dim">
                    {conn.triggers.length}T · {conn.actions.length}A
                  </p>
                </button>
              );
            }}
          </For>
        </div>
      </section>

      {/* ── SQUAD ── */}
      <section>
        <div class="mb-1 flex items-center justify-between">
          <h3 class="colony-label">SQUAD ({agents().length} agents)</h3>
          <button
            type="button"
            onClick={addAgent}
            class="colony-text-3xs border border-bark bg-soil-light px-2 py-0.5 text-text-secondary hover:border-bark-light"
          >
            + Add Agent
          </button>
        </div>

        <Show when={agents().length === 0}>
          <p class="colony-text-2xs py-3 text-center text-text-dim">
            No agents yet. Click "+ Add Agent" or describe your task above.
          </p>
        </Show>

        <div class="space-y-2">
          <For each={agents()}>
            {(agent) => {
              const isEditing = () => editingSlot() === agent.id;
              return (
                <div class="rounded border-2 border-bark bg-soil-deep p-2">
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-2">
                      <span class="colony-text-2xs font-bold text-text-primary">
                        {agent.connectorName
                          ? agent.connectorName.replace("connector-", "")
                          : "Unconfigured"}
                      </span>
                      <Show when={agent.triggerName}>
                        <span class="colony-text-3xs text-text-dim">
                          {agent.triggerName} → {agent.actionName}
                        </span>
                      </Show>
                    </div>
                    <div class="flex gap-1">
                      <button
                        type="button"
                        onClick={() => setEditingSlot(isEditing() ? null : agent.id)}
                        class="colony-text-3xs border border-bark px-1.5 py-0.5 text-text-secondary hover:border-bark-light"
                      >
                        {isEditing() ? "Done" : "Configure"}
                      </button>
                      <button
                        type="button"
                        onClick={() => removeAgent(agent.id)}
                        class="colony-text-3xs border border-bark px-1.5 py-0.5 text-status-error hover:border-status-error"
                      >
                        ✕
                      </button>
                    </div>
                  </div>

                  {/* Expanded config */}
                  <Show when={isEditing()}>
                    <div class="mt-2 space-y-2 border-t border-bark pt-2">
                      <label class="block">
                        <span class="colony-text-3xs text-text-secondary">Trigger connector</span>
                        <select
                          value={agent.connectorName}
                          onChange={(e) => {
                            updateAgent(agent.id, "connectorName", e.currentTarget.value);
                            updateAgent(agent.id, "triggerName", "");
                          }}
                          class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
                        >
                          <option value="">Select service...</option>
                          <For each={selectedConnectorNames()}>
                            {(name) => (
                              <option value={name}>{name.replace("connector-", "")}</option>
                            )}
                          </For>
                        </select>
                      </label>
                      <Show when={agent.connectorName}>
                        <label class="block">
                          <span class="colony-text-3xs text-text-secondary">Trigger event</span>
                          <select
                            value={agent.triggerName}
                            onChange={(e) =>
                              updateAgent(agent.id, "triggerName", e.currentTarget.value)
                            }
                            class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
                          >
                            <option value="">Select trigger...</option>
                            <For each={triggersForConnector(agent.connectorName)}>
                              {(t) => <option value={t.name}>{t.name}</option>}
                            </For>
                          </select>
                        </label>
                      </Show>
                      <label class="block">
                        <span class="colony-text-3xs text-text-secondary">Action connector</span>
                        <select
                          value={agent.actionConnector}
                          onChange={(e) => {
                            updateAgent(agent.id, "actionConnector", e.currentTarget.value);
                            updateAgent(agent.id, "actionName", "");
                          }}
                          class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
                        >
                          <option value="">Select service...</option>
                          <For each={selectedConnectorNames()}>
                            {(name) => (
                              <option value={name}>{name.replace("connector-", "")}</option>
                            )}
                          </For>
                        </select>
                      </label>
                      <Show when={agent.actionConnector}>
                        <label class="block">
                          <span class="colony-text-3xs text-text-secondary">Action</span>
                          <select
                            value={agent.actionName}
                            onChange={(e) =>
                              updateAgent(agent.id, "actionName", e.currentTarget.value)
                            }
                            class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
                          >
                            <option value="">Select action...</option>
                            <For each={actionsForConnector(agent.actionConnector)}>
                              {(a) => <option value={a.name}>{a.name}</option>}
                            </For>
                          </select>
                        </label>
                      </Show>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </section>

      {/* ── TEAM OVERVIEW ── */}
      <section class="rounded border-2 border-bark-light bg-soil-deep p-3">
        <h3 class="colony-label mb-2">TEAM OVERVIEW</h3>
        <div class="space-y-2">
          <label class="block">
            <span class="colony-text-3xs text-text-secondary">Team name</span>
            <input
              type="text"
              value={teamName()}
              onInput={(e) => setTeamName(e.currentTarget.value)}
              placeholder="e.g., GitHub Monitor"
              class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
            />
          </label>
          <div class="flex gap-3">
            <label class="block flex-1">
              <span class="colony-text-3xs text-text-secondary">Intent</span>
              <select
                value={intent()}
                onChange={(e) => setIntent(e.currentTarget.value)}
                class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
              >
                <For each={props.intents}>{(i) => <option value={i.value}>{i.label}</option>}</For>
              </select>
            </label>
            <div class="flex items-end">
              <button
                type="button"
                onClick={() => setGuardMode(!guardMode())}
                class={`colony-text-2xs border-2 px-3 py-1.5 ${
                  guardMode()
                    ? "border-status-ok bg-status-ok/10 text-status-ok"
                    : "border-bark bg-soil-mid text-text-dim"
                }`}
              >
                Guard {guardMode() ? "ON" : "OFF"}
              </button>
            </div>
          </div>
          <div class="colony-text-3xs text-text-dim">
            {agents().length} agent{agents().length !== 1 ? "s" : ""} •{" "}
            {[
              ...new Set(
                agents()
                  .map((a) => a.connectorName)
                  .filter(Boolean),
              ),
            ]
              .map((n) => n.replace("connector-", ""))
              .join(", ") || "no services"}{" "}
            • {intent()} intent
          </div>
        </div>
      </section>

      {/* ── DEPLOY ── */}
      <div class="flex justify-between">
        <button
          type="button"
          onClick={props.onCancel}
          class="colony-text-2xs text-text-dim hover:text-text-secondary"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={handleDeploy}
          disabled={!canDeploy() || deploying()}
          class="colony-text-2xs border-2 border-status-ok bg-soil-light px-4 py-2 font-bold text-status-ok hover:bg-soil-deep disabled:opacity-50"
        >
          {deploying() ? "Deploying..." : "Deploy Team"}
        </button>
      </div>
    </div>
  );
};
