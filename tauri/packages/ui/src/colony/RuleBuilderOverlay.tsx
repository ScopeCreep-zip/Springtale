/**
 * RuleBuilderOverlay — G5e visual rule builder, colony surface.
 *
 * Per `.claude/phases/phase-2b.md` "Visual Rule Builder":
 * - Enumerates available triggers from installed connector manifests
 * - Enumerates actions from connector `ActionDecl` schemas
 * - Form-based composition (the spec's drag-drop is a UX enhancement
 *   on top of this; what it requires is "Visual builder produces the
 *   same files the CLI creates", which the form path satisfies)
 * - Output: same TOML format as hand-authored rules
 * - "Save" button: writes via `provider.createConnectorRule`
 *
 * Composes the existing `TriggerPicker` / `ConditionEditor` /
 * `ActionPicker` / `RulePreview` shared components into one guided
 * overlay so the rule-builder surface lives alongside the rest of
 * the colony shell pattern (same `colony-modal` chrome as
 * `MemberPickerOverlay`, `AiConfigPanel`, etc.).
 *
 * Unlike `ConnectorConfigPanel`'s inline new-rule form (which
 * scopes to one connector at a time), this overlay starts unscoped
 * — the survivor picks the trigger connector + action connector
 * independently so cross-connector rules ("Telegram message → Slack
 * post") are first-class.
 */
import { createMemo, createSignal, createResource, Show } from "solid-js";
import type { Component } from "solid-js";
import { TriggerPicker } from "../TriggerPicker";
import { ActionPicker } from "../ActionPicker";
import { ConditionEditor } from "../ConditionEditor";
import { RulePreview } from "../RulePreview";
import type { ConditionDef } from "../ConditionEditor";
import { useDashboard } from "../dashboard/context";
import { useI18n } from "../i18n/context";

export interface RuleBuilderOverlayProps {
  /** Close the overlay without saving. */
  onCancel: () => void;
  /** Optional — fired after a rule successfully persists. */
  onSaved?: (ruleId: string) => void;
}

/**
 * Generate TOML matching the format the CLI writes. Keeps the
 * "no lock-in" property from the phase-2b spec: a survivor can
 * copy this TOML into a `.toml` file and hand-edit it later.
 */
function generateToml(args: {
  name: string;
  triggerConnector: string;
  triggerName: string;
  actionConnector: string;
  actionName: string;
  conditions: ConditionDef[];
}): string {
  if (!args.name || !args.triggerConnector || !args.triggerName) return "";
  const conditionsToml = args.conditions
    .map((c) => {
      const entries = Object.entries(c)
        .filter(([k]) => k !== "type")
        .filter(([, v]) => v !== undefined && v !== "")
        .map(([k, v]) => {
          if (Array.isArray(v)) return `${k} = [${v.join(", ")}]`;
          if (typeof v === "string") return `${k} = "${v}"`;
          return `${k} = ${JSON.stringify(v)}`;
        })
        .join(", ");
      return `[[conditions.${c.type}]]\n${entries}`;
    })
    .join("\n\n");
  return [
    `[rule]`,
    `name = "${args.name}"`,
    ``,
    `[trigger]`,
    `connector = "${args.triggerConnector}"`,
    `event = "${args.triggerName}"`,
    ``,
    `[action]`,
    `connector = "${args.actionConnector}"`,
    `name = "${args.actionName}"`,
    ...(conditionsToml ? ["", conditionsToml] : []),
  ].join("\n");
}

export const RuleBuilderOverlay: Component<RuleBuilderOverlayProps> = (props) => {
  const db = useDashboard();
  const { t } = useI18n();

  const [name, setName] = createSignal("");
  const [triggerConnector, setTriggerConnector] = createSignal("");
  const [triggerName, setTriggerName] = createSignal("");
  const [actionConnector, setActionConnector] = createSignal("");
  const [actionName, setActionName] = createSignal("");
  const [conditions, setConditions] = createSignal<ConditionDef[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Fetch the rule schema once so ConditionEditor can render
  // available condition types. Same path the orphan
  // `pages/RuleBuilder.tsx` used; provider surfaces it via
  // `getRuleSchema()`.
  const [schema] = createResource(async () => {
    try {
      return await db.provider.getRuleSchema();
    } catch (e) {
      setError(String(e));
      return null;
    }
  });

  const conditionTypes = (): string[] => {
    const s = schema();
    if (!s) return [];
    const cond = (s as Record<string, unknown>).conditions as Record<string, unknown> | undefined;
    return cond ? Object.keys(cond) : [];
  };

  const canSave = () =>
    name().trim().length > 0
    && triggerConnector().length > 0
    && triggerName().length > 0
    && actionConnector().length > 0
    && actionName().length > 0;

  // Reactive TOML preview — recomputes on any input change so the
  // survivor can see exactly what file the CLI would write.
  const previewToml = createMemo(() => generateToml({
    name: name(),
    triggerConnector: triggerConnector(),
    triggerName: triggerName(),
    actionConnector: actionConnector(),
    actionName: actionName(),
    conditions: conditions(),
  }));

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const ruleId = await db.provider.createConnectorRule({
        name: name().trim(),
        trigger_connector: triggerConnector(),
        trigger_event: triggerName(),
        action_connector: actionConnector(),
        action_name: actionName(),
        conditions: conditions(),
      });
      props.onSaved?.(ruleId);
      props.onCancel();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="colony-modal mx-auto max-w-3xl overflow-y-auto rounded border-2 border-bark bg-soil-mid p-6">
      <div class="mb-4 flex items-center justify-between">
        <h2 class="colony-text-md font-bold text-text-primary">
          {t("rules.builderTitle")}
        </h2>
        <button onClick={props.onCancel} class="colony-close-btn">✕</button>
      </div>

      <Show when={error()}>
        <div role="alert" aria-live="assertive"
             class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error">
          {error()}
        </div>
      </Show>

      <div class="space-y-4">
        {/* Name */}
        <section>
          <h3 class="colony-label mb-1">Name</h3>
          <input
            type="text"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
            placeholder="e.g. Alert on Kick live"
            class="colony-text-2xs w-full border-2 border-bark bg-soil-deep px-2 py-1.5 text-text-primary"
          />
        </section>

        {/* Trigger */}
        <section>
          <h3 class="colony-label mb-1">{t("rules.trigger")}</h3>
          <TriggerPicker
            connectors={db.schemas()}
            onSelect={(connector, trigger) => {
              setTriggerConnector(connector);
              setTriggerName(trigger);
            }}
          />
        </section>

        {/* Conditions */}
        <section>
          <h3 class="colony-label mb-1">{t("rules.conditions")}</h3>
          <ConditionEditor
            conditions={conditions()}
            conditionTypes={conditionTypes()}
            onChange={setConditions}
          />
        </section>

        {/* Action */}
        <section>
          <h3 class="colony-label mb-1">{t("rules.action")}</h3>
          <ActionPicker
            connectors={db.schemas()}
            onSelect={(connector, action) => {
              setActionConnector(connector);
              setActionName(action);
            }}
          />
        </section>

        {/* Preview — generates same TOML the CLI writes; surfaces
            the "no lock-in" property from the phase-2b spec. */}
        <Show when={previewToml().length > 0}>
          <section>
            <h3 class="colony-label mb-1">{t("rules.preview")}</h3>
            <RulePreview toml={previewToml()} />
          </section>
        </Show>

        <div class="flex gap-2 pt-2">
          <button
            onClick={save}
            disabled={!canSave() || saving()}
            class="colony-text-2xs border-2 border-status-ok bg-soil-light px-3 py-1.5 text-status-ok hover:bg-soil-deep disabled:opacity-50"
          >
            {saving() ? "Saving…" : t("common.save")}
          </button>
          <button
            onClick={props.onCancel}
            class="colony-text-2xs border-2 border-bark bg-soil-light px-3 py-1.5 text-text-secondary hover:bg-soil-deep"
          >
            {t("common.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
};
