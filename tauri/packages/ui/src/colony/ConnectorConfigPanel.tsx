import { createSignal, onMount, For, Show } from "solid-js";
import type { Component } from "solid-js";
import type { ConnectorSchema } from "@springtale/types";
import type { RuleItem } from "../Roster";
import { ConditionEditor } from "../ConditionEditor";
import type { ConditionDef } from "../ConditionEditor";
import type { ConfigSchema, ConfigSchemaProperty } from "@springtale/types";

export interface ConnectorConfigPanelProps {
  connectorId: string;
  schemas: ConnectorSchema[];
  rules: RuleItem[];
  currentConfig: unknown;
  /** JSON Schema describing config fields. When present, renders typed form instead of raw JSON. */
  configSchema?: ConfigSchema;
  onSave: (connectorId: string, config: Record<string, unknown>) => Promise<void>;
  onToggleRule: (ruleId: string, enabled: boolean) => Promise<void>;
  onDeleteRule: (ruleId: string) => Promise<void>;
  onCreateRule: (rule: { name: string; trigger_connector: string; trigger_event: string; action_connector: string; action_name: string; conditions?: unknown[] }) => Promise<void>;
  onTest: (connectorId: string) => Promise<void>;
  onClose: () => void;
  /** Condition type names from backend schema. */
  conditionTypes: string[];
}

/**
 * Full connector management panel.
 *
 * Per product model: config is per-connector. This panel lets
 * users manage credentials, view/toggle/delete rules, and
 * create new rules — all for a single connector.
 */
export const ConnectorConfigPanel: Component<ConnectorConfigPanelProps> = (props) => {
  const schema = () => props.schemas.find((s) => s.name === props.connectorId);
  const [configFields, setConfigFields] = createSignal<Record<string, unknown>>({});
  const [configText, setConfigText] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [testing, setTesting] = createSignal(false);
  const [error, setError] = createSignal("");
  const [testResult, setTestResult] = createSignal("");
  const [showNewRule, setShowNewRule] = createSignal(false);
  const [showRawJson, setShowRawJson] = createSignal(false);

  // New rule form state
  const [newRuleName, setNewRuleName] = createSignal("");
  const [triggerName, setTriggerName] = createSignal("");
  const [actionConnector, setActionConnector] = createSignal("");
  const [actionName, setActionName] = createSignal("");
  const [conditions, setConditions] = createSignal<ConditionDef[]>([]);

  /** Whether to use schema-driven form (vs raw JSON). */
  const hasConfigSchema = () => !!props.configSchema?.properties;

  onMount(() => {
    const current = props.currentConfig;
    const obj = (current && typeof current === "object" && current !== null ? current : {}) as Record<string, unknown>;
    setConfigText(JSON.stringify(obj, null, 2));

    // Pre-populate form fields from current config + schema defaults
    if (props.configSchema?.properties) {
      const fields: Record<string, unknown> = { ...obj };
      for (const [key, prop] of Object.entries(props.configSchema.properties)) {
        if (fields[key] === undefined && prop.default !== undefined) {
          fields[key] = prop.default;
        }
      }
      setConfigFields(fields);
    }
  });

  const getFieldValue = (key: string): unknown => {
    return configFields()[key] ?? "";
  };

  const setFieldValue = (key: string, value: unknown) => {
    setConfigFields((prev) => ({ ...prev, [key]: value }));
  };

  const buildConfigFromFields = (): Record<string, unknown> => {
    const result: Record<string, unknown> = {};
    const cs = props.configSchema;
    if (!cs?.properties) return result;

    for (const [key, prop] of Object.entries(cs.properties)) {
      const val = configFields()[key];
      // Skip empty optional fields
      if (val === "" || val === undefined || val === null) {
        if (cs.required?.includes(key)) {
          result[key] = val;
        }
        continue;
      }
      // Coerce types
      if (prop.type === "integer" && typeof val === "string") {
        const num = parseInt(val, 10);
        if (!isNaN(num)) result[key] = num;
      } else if (prop.type === "boolean" && typeof val === "string") {
        result[key] = val === "true";
      } else if (prop.type === "array" && typeof val === "string") {
        result[key] = val.split(",").map((s) => s.trim()).filter(Boolean);
      } else {
        result[key] = val;
      }
    }
    return result;
  };

  const handleSave = async () => {
    setSaving(true);
    setError("");
    try {
      const config = hasConfigSchema() && !showRawJson()
        ? buildConfigFromFields()
        : JSON.parse(configText());
      await props.onSave(props.connectorId, config);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult("");
    try {
      await props.onTest(props.connectorId);
      setTestResult("Test completed");
    } catch (e) {
      setTestResult(`Test failed: ${e}`);
    } finally {
      setTesting(false);
    }
  };

  const handleCreateRule = async () => {
    if (!newRuleName() || !triggerName()) return;
    setError("");
    try {
      await props.onCreateRule({
        name: newRuleName(),
        trigger_connector: props.connectorId,
        trigger_event: triggerName(),
        action_connector: actionConnector() || props.connectorId,
        action_name: actionName(),
        conditions: conditions(),
      });
      setShowNewRule(false);
      setNewRuleName("");
      setTriggerName("");
      setActionConnector("");
      setActionName("");
      setConditions([]);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div class="mx-auto max-w-lg overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6" style={{ "max-height": "80vh" }}>
      <div class="mb-4 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">{props.connectorId}</h2>
        <button onClick={props.onClose} class="colony-close-btn">✕</button>
      </div>

      {error() && (
        <div class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error">{error()}</div>
      )}

      {/* Section 1: Configuration */}
      <section class="mb-4">
        <div class="mb-2 flex items-center justify-between">
          <h3 class="colony-label">CONFIGURATION</h3>
          <Show when={hasConfigSchema()}>
            <button
              onClick={() => {
                if (!showRawJson()) {
                  // Sync fields → JSON before switching
                  setConfigText(JSON.stringify(buildConfigFromFields(), null, 2));
                }
                setShowRawJson(!showRawJson());
              }}
              class="colony-text-3xs border border-bark px-2 py-0.5 text-text-dim hover:border-bark-light"
            >
              {showRawJson() ? "Form View" : "JSON View"}
            </button>
          </Show>
        </div>
        <Show when={schema()}>
          {(s) => <p class="colony-text-3xs mb-2 text-text-dim">{s().description || props.connectorId}</p>}
        </Show>

        {/* Schema-driven form fields */}
        <Show when={hasConfigSchema() && !showRawJson()} fallback={
          <textarea
            value={configText()}
            onInput={(e) => setConfigText(e.currentTarget.value)}
            rows={6}
            class="colony-text-2xs w-full border-2 border-bark bg-soil-deep p-2 text-text-primary focus:border-accent focus:outline-none"
            style={{ "font-family": "monospace", resize: "vertical" }}
          />
        }>
          <div class="space-y-3">
            <For each={Object.entries(props.configSchema?.properties ?? {})}>
              {([key, prop]: [string, ConfigSchemaProperty]) => {
                const isRequired = props.configSchema?.required?.includes(key) ?? false;
                const isSecret = prop["x-secret"] === true;

                return (
                  <div>
                    <label class="colony-text-3xs text-text-secondary">
                      {key}
                      {isRequired && <span class="ml-0.5 text-status-error">*</span>}
                    </label>
                    <Show when={prop.description}>
                      <p class="colony-text-3xs text-text-dim">{prop.description}</p>
                    </Show>

                    {/* Boolean field */}
                    <Show when={prop.type === "boolean"}>
                      <div class="mt-1 flex items-center gap-2">
                        <button
                          onClick={() => setFieldValue(key, !getFieldValue(key))}
                          class={`colony-text-2xs border-2 px-3 py-1 ${
                            getFieldValue(key)
                              ? "border-status-ok bg-status-ok/10 text-status-ok"
                              : "border-bark bg-soil-deep text-text-dim"
                          }`}
                        >
                          {getFieldValue(key) ? "ON" : "OFF"}
                        </button>
                      </div>
                    </Show>

                    {/* Enum select */}
                    <Show when={prop.type === "string" && prop.enum}>
                      <select
                        value={String(getFieldValue(key) ?? prop.default ?? "")}
                        onChange={(e) => setFieldValue(key, e.currentTarget.value)}
                        class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
                      >
                        <For each={prop.enum ?? []}>
                          {(opt) => <option value={opt}>{opt}</option>}
                        </For>
                      </select>
                    </Show>

                    {/* Secret string field — password input */}
                    <Show when={isSecret && prop.type === "string" && !prop.enum}>
                      <input
                        type="password"
                        value={String(getFieldValue(key) ?? "")}
                        onInput={(e) => setFieldValue(key, e.currentTarget.value)}
                        placeholder={prop.default !== undefined ? String(prop.default) : `Enter ${key}...`}
                        class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                      />
                    </Show>

                    {/* Regular string field */}
                    <Show when={!isSecret && prop.type === "string" && !prop.enum}>
                      <input
                        type="text"
                        value={String(getFieldValue(key) ?? "")}
                        onInput={(e) => setFieldValue(key, e.currentTarget.value)}
                        placeholder={prop.default !== undefined ? String(prop.default) : `Enter ${key}...`}
                        class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                      />
                    </Show>

                    {/* Integer field */}
                    <Show when={prop.type === "integer"}>
                      <input
                        type="number"
                        value={String(getFieldValue(key) ?? prop.default ?? "")}
                        onInput={(e) => setFieldValue(key, e.currentTarget.value)}
                        placeholder={prop.default !== undefined ? String(prop.default) : "0"}
                        class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                      />
                    </Show>

                    {/* Array field — comma-separated */}
                    <Show when={prop.type === "array"}>
                      <input
                        type="text"
                        value={Array.isArray(getFieldValue(key))
                          ? (getFieldValue(key) as string[]).join(", ")
                          : String(getFieldValue(key) ?? "")}
                        onInput={(e) => setFieldValue(key, e.currentTarget.value)}
                        placeholder="Comma-separated values..."
                        class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none"
                      />
                      <p class="colony-text-3xs mt-0.5 text-text-dim">Separate multiple values with commas</p>
                    </Show>

                    {/* Object field — raw JSON */}
                    <Show when={prop.type === "object"}>
                      <textarea
                        value={typeof getFieldValue(key) === "object"
                          ? JSON.stringify(getFieldValue(key), null, 2)
                          : String(getFieldValue(key) ?? "{}")}
                        onInput={(e) => {
                          try { setFieldValue(key, JSON.parse(e.currentTarget.value)); }
                          catch { /* let them keep typing */ }
                        }}
                        rows={3}
                        class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-deep p-2 text-text-primary focus:border-accent focus:outline-none"
                        style={{ "font-family": "monospace", resize: "vertical" }}
                      />
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>

        <div class="mt-2 flex gap-2">
          <button onClick={handleSave} disabled={saving()}
            class="colony-text-3xs border-2 border-status-ok bg-soil-light px-3 py-1 text-status-ok hover:bg-soil-deep disabled:opacity-50">
            {saving() ? "Saving..." : "Save"}
          </button>
          <button onClick={handleTest} disabled={testing()}
            class="colony-text-3xs border-2 border-bark bg-soil-light px-3 py-1 text-text-secondary hover:bg-soil-deep disabled:opacity-50">
            {testing() ? "Testing..." : "Test"}
          </button>
        </div>
        {testResult() && (
          <p class={`colony-text-3xs mt-1 ${testResult().includes("failed") ? "text-status-error" : "text-status-ok"}`}>{testResult()}</p>
        )}
      </section>

      {/* Section 2: Available Triggers & Actions */}
      <Show when={schema()}>
        {(s) => (
          <section class="mb-4">
            <Show when={s().triggers.length > 0}>
              <h3 class="colony-label mb-1">TRIGGERS ({s().triggers.length})</h3>
              <div class="mb-2 space-y-0.5">
                <For each={s().triggers}>
                  {(t) => (
                    <button
                      class="colony-text-2xs flex w-full items-center gap-2 border-b border-bark py-1 text-start hover:bg-soil-light"
                      onClick={() => { setShowNewRule(true); setTriggerName(t.name); setNewRuleName(`${props.connectorId} ${t.name}`); }}
                    >
                      <span class="text-status-ok">*</span>
                      <span class="text-text-primary">{t.name}</span>
                      <Show when={t.description}><span class="truncate text-text-dim">— {t.description}</span></Show>
                      <span class="colony-text-3xs ml-auto shrink-0 text-text-dim">+ rule</span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
            <Show when={s().actions.length > 0}>
              <h3 class="colony-label mb-1">ACTIONS ({s().actions.length})</h3>
              <div class="space-y-0.5">
                <For each={s().actions}>
                  {(a) => (
                    <button
                      class="colony-text-2xs flex w-full items-center gap-2 border-b border-bark py-1 text-start hover:bg-soil-light"
                      onClick={() => { setShowNewRule(true); setActionConnector(props.connectorId); setActionName(a.name); }}
                    >
                      <span class="text-mycelium-active">~</span>
                      <span class="text-text-primary">{a.name}</span>
                      <Show when={a.description}><span class="truncate text-text-dim">— {a.description}</span></Show>
                      <span class="colony-text-3xs ml-auto shrink-0 text-text-dim">+ rule</span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </section>
        )}
      </Show>

      {/* Section 3: Active Rules */}
      <section class="mb-4">
        <div class="mb-1 flex items-center justify-between">
          <h3 class="colony-label">RULES ({props.rules.length})</h3>
          <button onClick={() => setShowNewRule(!showNewRule())}
            class="colony-text-3xs border border-bark bg-soil-light px-2 py-0.5 text-text-secondary hover:border-bark-light">
            {showNewRule() ? "Cancel" : "+ New Rule"}
          </button>
        </div>
        <Show when={props.rules.length > 0}>
          <div class="space-y-1">
            <For each={props.rules}>
              {(rule) => (
                <div class="flex items-center gap-2 rounded border border-bark p-1.5">
                  <span class={`inline-block h-2 w-2 shrink-0 rounded-full ${
                    rule.status === "enabled" ? "bg-status-ok" : "bg-status-idle"
                  }`} />
                  <span class="colony-text-2xs flex-1 truncate font-bold text-text-primary">{rule.name}</span>
                  <span class="colony-text-3xs text-text-dim">{rule.triggerType}</span>
                  <button onClick={() => props.onToggleRule(rule.id, rule.status === "enabled")}
                    class="colony-text-3xs border border-bark px-1.5 py-0.5 text-text-secondary hover:border-bark-light">
                    {rule.status === "enabled" ? "Pause" : "Enable"}
                  </button>
                  <button onClick={() => props.onDeleteRule(rule.id)}
                    class="colony-text-3xs border border-bark px-1.5 py-0.5 text-status-error hover:border-status-error">
                    Delete
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
        <Show when={props.rules.length === 0 && !showNewRule()}>
          <p class="colony-text-2xs py-2 text-text-dim">No rules. Click + New Rule to create one.</p>
        </Show>
      </section>

      {/* Section 4: Create New Rule */}
      <Show when={showNewRule()}>
        <section class="rounded border-2 border-bark-light bg-soil-deep p-3">
          <h3 class="colony-label mb-2">NEW RULE</h3>
          <div class="space-y-3">
            <div>
              <label class="colony-text-3xs text-text-secondary">Rule Name</label>
              <input type="text" value={newRuleName()} onInput={(e) => setNewRuleName(e.currentTarget.value)}
                placeholder="e.g., Monitor file changes"
                class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary placeholder-text-dim focus:border-accent focus:outline-none" />
            </div>
            <div>
              <label class="colony-text-3xs text-text-secondary">Trigger</label>
              <select
                value={triggerName()}
                onChange={(e) => setTriggerName(e.currentTarget.value)}
                class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
              >
                <option value="">Select trigger...</option>
                <For each={schema()?.triggers ?? []}>
                  {(t) => <option value={t.name}>{t.name}{t.description ? ` — ${t.description}` : ""}</option>}
                </For>
              </select>
            </div>
            <div>
              <label class="colony-text-3xs text-text-secondary">Action</label>
              <select
                value={actionName()}
                onChange={(e) => { setActionName(e.currentTarget.value); setActionConnector(props.connectorId); }}
                class="colony-text-2xs mt-0.5 w-full border-2 border-bark bg-soil-mid px-2 py-1.5 text-text-primary"
              >
                <option value="">Select action...</option>
                <For each={schema()?.actions ?? []}>
                  {(a) => <option value={a.name}>{a.name}{a.description ? ` — ${a.description}` : ""}</option>}
                </For>
              </select>
            </div>
            <div>
              <label class="colony-text-3xs text-text-secondary">Conditions (optional)</label>
              <div class="mt-0.5">
                <ConditionEditor conditions={conditions()} conditionTypes={props.conditionTypes} onChange={setConditions} />
              </div>
            </div>
            <button onClick={handleCreateRule} disabled={!newRuleName() || !triggerName()}
              class="colony-text-2xs border-2 border-status-ok bg-soil-light px-3 py-1.5 text-status-ok hover:bg-soil-deep disabled:opacity-50">
              Create Rule
            </button>
          </div>
        </section>
      </Show>
    </div>
  );
};
