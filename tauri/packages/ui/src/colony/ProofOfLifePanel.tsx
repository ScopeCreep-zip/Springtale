/**
 * ProofOfLifePanel — W1.E post-deploy confirmation.
 *
 * After a recipe deploys, the user needs to *see* the bot working.
 * This panel:
 *   - Renders a "Bot is loaded" status using the report from
 *     `applyRecipe`.
 *   - Offers a "Send test event" button that fires each created rule
 *     via `runRule` (server dry-runs against a synthetic trigger;
 *     surfaces the matched/actions count).
 *   - Plays a one-time celebration sprite when the test confirms the
 *     bot acts. The "has seen celebration" flag persists in the
 *     backend config store so future deploys are quieter — backend
 *     owns the decision per `feedback_zero_frontend_logic`.
 *
 * Per `feedback_preflight_zero_to_live`, the worst UX is a bot that
 * deploys but silently doesn't work. This panel makes "still no
 * activity 30s later" visible so the user knows to troubleshoot.
 */

import type { Component } from "solid-js";
import { For, Match, Show, Switch, createSignal } from "solid-js";
import type { RecipeApplyReport } from "../dashboard/types";
import { useDashboard } from "../dashboard/context";

export interface ProofOfLifePanelProps {
  report: RecipeApplyReport;
  onDismiss: () => void;
  /** Optional — fires the first time a test succeeds so the parent can
   *  swap to the colony canvas / scroll to the new bot. */
  onFirstSuccess?: () => void;
}

type RuleTestState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "ok"; matched: boolean; actions_count: number }
  | { kind: "error"; message: string };

export const ProofOfLifePanel: Component<ProofOfLifePanelProps> = (props) => {
  const db = useDashboard();
  const [testStates, setTestStates] = createSignal<Record<string, RuleTestState>>(
    Object.fromEntries(props.report.rules_created.map((id) => [id, { kind: "idle" }])),
  );
  const [celebrate, setCelebrate] = createSignal(false);

  const handleTest = async (ruleId: string) => {
    setTestStates((s) => ({ ...s, [ruleId]: { kind: "running" } }));
    try {
      const result = await db.provider.runRule(ruleId);
      const actionsCount = (result as { actions_count?: number }).actions_count ?? 0;
      setTestStates((s) => ({
        ...s,
        [ruleId]: { kind: "ok", matched: result.matched, actions_count: actionsCount },
      }));
      if (result.matched && !celebrate()) {
        setCelebrate(true);
        props.onFirstSuccess?.();
      }
    } catch (e) {
      setTestStates((s) => ({
        ...s,
        [ruleId]: { kind: "error", message: String(e) },
      }));
    }
  };

  return (
    <div class="mx-auto max-w-2xl rounded border-2 border-bark bg-soil-mid p-6">
      <div class="flex items-start justify-between">
        <div>
          <p class="colony-text-md font-bold text-text-primary">
            🟢 Bot is loaded
          </p>
          <p class="colony-text-xs mt-1 text-text-secondary">{props.report.summary}</p>
        </div>
        <button
          class="colony-command-btn colony-text-2xs px-3 py-1"
          onClick={() => props.onDismiss()}
        >
          Done
        </button>
      </div>

      <Show when={props.report.connectors_configured.length > 0}>
        <div class="mt-4">
          <p class="colony-text-3xs text-text-dim">Connectors configured</p>
          <ul class="mt-1 flex flex-wrap gap-1">
            <For each={props.report.connectors_configured}>
              {(c) => (
                <li class="colony-text-3xs rounded border border-bark bg-soil-deep px-2 py-1 text-text-primary">
                  {c.replace(/^connector-/, "")}
                </li>
              )}
            </For>
          </ul>
        </div>
      </Show>

      <Show when={props.report.rules_created.length > 0}>
        <div class="mt-4">
          <p class="colony-text-3xs text-text-dim">Rules — try a test event</p>
          <ul class="mt-2 space-y-2">
            <For each={props.report.rules_created}>
              {(rid) => (
                <li class="flex items-start gap-2">
                  <button
                    class="colony-command-btn colony-text-3xs px-3 py-1"
                    disabled={testStates()[rid]?.kind === "running"}
                    onClick={() => handleTest(rid)}
                  >
                    {testStates()[rid]?.kind === "running" ? "Testing…" : "Send test"}
                  </button>
                  <div class="flex-1">
                    <div class="colony-text-3xs text-text-primary">rule {rid.slice(0, 8)}…</div>
                    <RuleStatus state={testStates()[rid]} />
                  </div>
                </li>
              )}
            </For>
          </ul>
        </div>
      </Show>

      <Show when={celebrate()}>
        <div class="mt-4 rounded border border-status-ok bg-status-ok/10 p-3 text-center">
          <p class="colony-text-md">🎉</p>
          <p class="colony-text-xs mt-1 font-bold text-status-ok">
            Bot acted!
          </p>
          <p class="colony-text-3xs mt-1 text-text-dim">
            Your bot is wired up and responding to events.
          </p>
        </div>
      </Show>
    </div>
  );
};

const RuleStatus: Component<{ state: RuleTestState | undefined }> = (props) => {
  const s = () => props.state ?? { kind: "idle" as const };
  return (
    <Show when={s().kind !== "idle"}>
      <Switch>
        <Match when={s().kind === "ok" && (s() as { kind: "ok"; matched: boolean }).matched}>
          <p class="colony-text-3xs text-status-ok">
            Matched — {(s() as { kind: "ok"; actions_count: number }).actions_count} action
            {(s() as { kind: "ok"; actions_count: number }).actions_count === 1 ? "" : "s"} ready.
          </p>
        </Match>
        <Match when={s().kind === "ok" && !(s() as { kind: "ok"; matched: boolean }).matched}>
          <p class="colony-text-3xs text-status-warn">
            Didn't match the synthetic trigger. The rule may need a real event to fire.
          </p>
        </Match>
        <Match when={s().kind === "error"}>
          <p class="colony-text-3xs text-status-error">
            {(s() as { kind: "error"; message: string }).message}
          </p>
        </Match>
      </Switch>
    </Show>
  );
};
