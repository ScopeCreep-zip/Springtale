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

import type { Component } from "solid-js";
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { ActionPicker } from "../ActionPicker";
import type { ConditionDef } from "../ConditionEditor";
import { ConditionEditor } from "../ConditionEditor";
import { useDashboard } from "../dashboard/context";
import { useI18n } from "../i18n/context";
import { RulePreview } from "../RulePreview";
import { TriggerPicker } from "../TriggerPicker";

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
  extraActions: { action_connector: string; action_name: string }[];
  matchAny: boolean;
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
  // W6: when any-of is chosen with 2+ conditions, the backend wraps them in
  // a single Or — note that here so the preview matches what gets saved.
  const conditionsHeader =
    args.matchAny && args.conditions.length > 1 ? "# matches ANY of the following:\n" : "";

  // W6 chain: filled-in extra steps render as additional [[action]] blocks
  // (the backend assembles these into one ordered Action::Chain).
  const steps = [
    { connector: args.actionConnector, name: args.actionName },
    ...args.extraActions
      .filter((s) => s.action_connector && s.action_name)
      .map((s) => ({ connector: s.action_connector, name: s.action_name })),
  ];
  const actionsToml = steps
    .map((s) => [`[[action]]`, `connector = "${s.connector}"`, `name = "${s.name}"`].join("\n"))
    .join("\n\n");

  return [
    `[rule]`,
    `name = "${args.name}"`,
    ``,
    `[trigger]`,
    `connector = "${args.triggerConnector}"`,
    `event = "${args.triggerName}"`,
    ``,
    actionsToml,
    ...(conditionsToml ? ["", conditionsHeader + conditionsToml] : []),
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
  // W6 all-of / any-of toggle for the condition set.
  const [matchAny, setMatchAny] = createSignal(false);
  // W6 chain composer — extra action steps after the primary action.
  const [extraActions, setExtraActions] = createSignal<
    { action_connector: string; action_name: string }[]
  >([]);
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
    name().trim().length > 0 &&
    triggerConnector().length > 0 &&
    triggerName().length > 0 &&
    actionConnector().length > 0 &&
    actionName().length > 0;

  // Reactive TOML preview — recomputes on any input change so the
  // survivor can see exactly what file the CLI would write.
  const previewToml = createMemo(() =>
    generateToml({
      name: name(),
      triggerConnector: triggerConnector(),
      triggerName: triggerName(),
      actionConnector: actionConnector(),
      actionName: actionName(),
      conditions: conditions(),
      extraActions: extraActions(),
      matchAny: matchAny(),
    }),
  );

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
        // W6 — only send chain steps that are fully filled in.
        extra_actions: extraActions().filter(
          (s) => s.action_connector.length > 0 && s.action_name.length > 0,
        ),
        match_any: matchAny(),
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
        <h2 class="colony-text-md font-bold text-text-primary">{t("rules.builderTitle")}</h2>
        <button type="button" onClick={props.onCancel} class="colony-close-btn">
          ✕
        </button>
      </div>

      <Show when={error()}>
        <div
          role="alert"
          aria-live="assertive"
          class="colony-text-2xs mb-3 border border-status-error bg-status-error/10 p-2 text-status-error"
        >
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
          <div class="mb-1 flex items-center justify-between">
            <h3 class="colony-label">{t("rules.conditions")}</h3>
            {/* W6 all-of / any-of — only meaningful with 2+ conditions. */}
            <Show when={conditions().length > 1}>
              <div class="flex gap-1">
                <button
                  type="button"
                  onClick={() => setMatchAny(false)}
                  classList={{ "text-status-ok": !matchAny(), "text-text-dim": matchAny() }}
                  class="colony-text-3xs border border-bark px-1.5 py-0.5"
                >
                  ALL OF
                </button>
                <button
                  type="button"
                  onClick={() => setMatchAny(true)}
                  classList={{ "text-status-ok": matchAny(), "text-text-dim": !matchAny() }}
                  class="colony-text-3xs border border-bark px-1.5 py-0.5"
                >
                  ANY OF
                </button>
              </div>
            </Show>
          </div>
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

          {/* W6 chain composer — "And then…" extra steps, run in order. */}
          <For each={extraActions()}>
            {(_step, i) => (
              <div class="mt-2 border-l-2 border-bark pl-2">
                <div class="mb-1 flex items-center justify-between">
                  <span class="colony-text-3xs text-text-dim">AND THEN</span>
                  <button
                    type="button"
                    class="colony-text-3xs text-status-warn"
                    onClick={() => setExtraActions((prev) => prev.filter((_, idx) => idx !== i()))}
                  >
                    ✕ remove
                  </button>
                </div>
                <ActionPicker
                  connectors={db.schemas()}
                  onSelect={(connector, action) => {
                    setExtraActions((prev) => {
                      const next = [...prev];
                      next[i()] = { action_connector: connector, action_name: action };
                      return next;
                    });
                  }}
                />
              </div>
            )}
          </For>

          <Show when={actionName().length > 0}>
            <button
              type="button"
              class="colony-text-3xs mt-2 border-2 border-bark bg-soil-light px-2 py-1 text-text-secondary hover:bg-soil-deep"
              onClick={() =>
                setExtraActions((prev) => [...prev, { action_connector: "", action_name: "" }])
              }
            >
              + And then…
            </button>
          </Show>
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
            type="button"
            onClick={save}
            disabled={!canSave() || saving()}
            class="colony-text-2xs border-2 border-status-ok bg-soil-light px-3 py-1.5 text-status-ok hover:bg-soil-deep disabled:opacity-50"
          >
            {saving() ? "Saving…" : t("common.save")}
          </button>
          <button
            type="button"
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
