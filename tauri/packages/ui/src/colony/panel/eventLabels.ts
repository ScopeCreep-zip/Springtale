import type { CooperationEvent } from "../../dashboard/types";

export const EVENT_LABELS: Record<CooperationEvent["kind"], string> = {
  intervention_fired: "INTERVENTION",
  pacing_phase_changed: "PACING",
  cascade_hit: "CASCADE",
  consensus_vote_opened: "VOTE OPEN",
  consensus_vote_resolved: "VOTE END",
  commit_phase_changed: "COMMIT",
  sacrifice_yield: "YIELD",
  role_transformed: "ROLE",
  member_marked_down: "DOWN",
  supervisor_escalated: "ESCALATE",
  recovery_action_taken: "RECOVERY",
  surface_deposited: "SURFACE",
  interference_detected: "INTERFERE",
  cfp_round_started: "CFP OPEN",
  cfp_round_resolved: "CFP END",
  cbba_replan_requested: "REPLAN",
  cbba_replan_resolved: "REPLAN END",
  utterance: "UTTER",
};

export function severityFor(kind: CooperationEvent["kind"]): "error" | "warn" | "ok" | "idle" {
  switch (kind) {
    case "intervention_fired":
    case "supervisor_escalated":
    case "cascade_hit":
    case "interference_detected":
      return "error";
    case "member_marked_down":
    case "pacing_phase_changed":
    case "consensus_vote_opened":
    case "cfp_round_started":
    case "cbba_replan_requested":
      return "warn";
    case "sacrifice_yield":
    case "recovery_action_taken":
    case "consensus_vote_resolved":
    case "cfp_round_resolved":
    case "cbba_replan_resolved":
    case "commit_phase_changed":
    case "role_transformed":
    case "surface_deposited":
      return "ok";
    default:
      return "idle";
  }
}

export function detailFor(event: CooperationEvent): string {
  switch (event.kind) {
    case "intervention_fired":
      return event.summary;
    case "pacing_phase_changed":
      return `${event.from} → ${event.to}`;
    case "cascade_hit":
      return `streak ${event.streak} | ${event.members_affected} affected`;
    case "consensus_vote_opened":
      return `${event.vote_id.slice(0, 8)} ${event.deadline_ms}ms`;
    case "consensus_vote_resolved":
      return `${event.vote_id.slice(0, 8)} → ${event.outcome}`;
    case "commit_phase_changed":
      return `${event.barrier_id.slice(0, 8)} → ${event.phase}`;
    case "sacrifice_yield":
      return `${event.sacrificer.slice(0, 8)} → ${event.beneficiary.slice(0, 8)} (${event.utility.toFixed(2)})`;
    case "role_transformed":
      return `${event.agent.slice(0, 8)} ${event.from} → ${event.to}`;
    case "member_marked_down":
      return `${event.agent.slice(0, 8)} tick ${event.since_tick}`;
    case "supervisor_escalated":
      return event.reason;
    case "recovery_action_taken":
      return `${event.helper.slice(0, 8)} → ${event.in_distress.slice(0, 8)} (${event.action})`;
    case "surface_deposited":
      return `${event.agent.slice(0, 8)} ${event.surface_kind} ${event.ttl_ms}ms`;
    case "interference_detected":
      return `${event.interference_kind} (${event.agents.length})`;
    case "cfp_round_started":
      return `${event.cfp_id.slice(0, 8)} ${event.capability}`;
    case "cfp_round_resolved":
      return `${event.cfp_id.slice(0, 8)} → ${event.winner?.slice(0, 8) ?? "no winner"}`;
    case "cbba_replan_requested":
      return event.reason;
    case "cbba_replan_resolved":
      return `${event.outcome.status} ${event.outcome.sweeps}s ${event.outcome.assigned}/${event.outcome.assigned + event.outcome.unassigned}`;
    case "utterance": {
      // Plan §1.15: name + who said it (member, or the standalone rule).
      const who = event.agent ?? event.rule_id ?? "formation";
      return `${event.utterance.utter} ${who.slice(0, 8)}`;
    }
  }
}
