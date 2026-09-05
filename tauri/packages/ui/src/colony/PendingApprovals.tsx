/**
 * PendingApprovals — plan 6.7. Renders the first pending chat-gate
 * approval as an ApprovalCard; approve / deny go through the provider
 * and the queue reloads. Renders nothing when nothing is pending.
 */
import { type Component, Show } from "solid-js";
import { useDashboard } from "../dashboard/context";
import type { ApprovalInfo } from "../dashboard/types";
import { ApprovalCard } from "./ApprovalCard";

function actionTypeOf(a: ApprovalInfo): string {
  if (typeof a.capability === "string") return a.capability;
  if (a.capability && "action_type" in a.capability) return String(a.capability.action_type);
  return Object.keys(a.capability ?? {})[0] ?? "unknown";
}

export const PendingApprovals: Component = () => {
  const db = useDashboard();
  const first = () => db.pendingApprovals()[0];
  return (
    <Show when={first()}>
      {(a) => (
        <ApprovalCard
          connectorName={a().connector_name}
          actionType={actionTypeOf(a())}
          rationale={`${a().summary} (${db.pendingApprovals().length} pending)`}
          expiresAt={a().expires_at}
          onDecision={(approve) => void db.resolveApproval(a().id, approve)}
        />
      )}
    </Show>
  );
};
