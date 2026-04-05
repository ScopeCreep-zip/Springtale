import { For, Show } from "solid-js";
import type { Component } from "solid-js";
import type { FormationDetail as FormationDetailType, FormationMember, MomentumTier } from "@springtale/types";
import { useI18n } from "./i18n/context";

export interface FormationDetailProps {
  formation: FormationDetailType;
  onDissolve?: () => void;
  onChangeIntent?: (intent: string) => void;
}

/**
 * Formation detail — shows full formation state in the command panel.
 *
 * Displays: all members with health/role/attention, momentum tier
 * with capability unlock indicator, intent pattern, and formation
 * controls (dissolve, change intent).
 *
 * Per COOPERATION.pdf: the user sees the formation as a commander
 * sees their army — aggregate status with ability to drill into
 * individual unit state.
 */
export const FormationDetailView: Component<FormationDetailProps> = (props) => {
  const { t } = useI18n();

  const healthLabel = (member: FormationMember): string => {
    switch (member.health.type) {
      case "Operational": return "●";
      case "Degraded": return "◐";
      case "Incapacitated": return "○";
      case "Dead": return "✕";
      default: return "?";
    }
  };

  const healthColor = (member: FormationMember): string => {
    switch (member.health.type) {
      case "Operational": return "text-green-400";
      case "Degraded": return "text-yellow-400";
      case "Incapacitated": return "text-red-400";
      case "Dead": return "text-gray-600";
      default: return "text-gray-400";
    }
  };

  const roleLabel = (member: FormationMember): string => {
    switch (member.current_role.type) {
      case "Primary": return member.current_role.task;
      case "Support": return "support";
      case "Information": return "intel";
      case "Custom": return member.current_role.name;
      default: return "—";
    }
  };

  const tierCapabilities = (tier: MomentumTier): string[] => {
    switch (tier) {
      case "Cold": return ["read env"];
      case "Warming": return ["read env", "neighbors", "chain"];
      case "Hot": return ["read env", "neighbors", "chain", "write env", "commit"];
      case "Fever": return ["read env", "neighbors", "chain", "write env", "commit", "consensus", "AI", "recruit"];
      default: return [];
    }
  };

  return (
    <div class="flex h-full flex-col overflow-hidden">
      <div class="flex items-center justify-between border-b border-gray-800 px-4 py-2">
        <div class="flex items-center gap-2">
          <span class="text-sm font-semibold text-white">{props.formation.intent}</span>
          <span class="rounded bg-gray-800 px-1.5 py-0.5 text-xs text-gray-400">
            {props.formation.momentum_tier}
          </span>
        </div>
        <Show when={props.onDissolve}>
          <button
            onClick={() => props.onDissolve?.()}
            class="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-900/30"
          >
            {t("common.delete")}
          </button>
        </Show>
      </div>

      <div class="flex flex-1 overflow-hidden">
        {/* Members list */}
        <div class="flex-1 overflow-y-auto p-3">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-gray-500">
            Agents ({props.formation.members.length})
          </h4>
          <ul class="mt-2 space-y-1">
            <For each={props.formation.members}>
              {(member) => (
                <li class="flex items-center justify-between rounded bg-gray-800/50 px-2 py-1.5 text-xs">
                  <div class="flex items-center gap-2">
                    <span class={healthColor(member)}>{healthLabel(member)}</span>
                    <span class="text-gray-300">{member.agent_id.substring(0, 8)}</span>
                    <span class="text-gray-500">{roleLabel(member)}</span>
                  </div>
                  <div class="flex items-center gap-2">
                    <span class="text-gray-600" title="attention load">
                      {Math.round(member.attention_load * 100)}%
                    </span>
                  </div>
                </li>
              )}
            </For>
          </ul>

          <h4 class="mt-4 text-xs font-semibold uppercase tracking-wider text-gray-500">
            Capabilities ({props.formation.momentum_tier})
          </h4>
          <div class="mt-1 flex flex-wrap gap-1">
            <For each={tierCapabilities(props.formation.momentum_tier)}>
              {(cap) => (
                <span class="rounded bg-gray-800 px-1.5 py-0.5 text-xs text-gray-400">
                  {cap}
                </span>
              )}
            </For>
          </div>
        </div>

        {/* Capabilities from each member */}
        <div class="w-48 border-s border-gray-800 overflow-y-auto p-3">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-gray-500">
            Resources
          </h4>
          <ul class="mt-2 space-y-1">
            <For each={props.formation.members}>
              {(member) => (
                <For each={member.capabilities}>
                  {(cap) => (
                    <li class="text-xs text-gray-400">{cap}</li>
                  )}
                </For>
              )}
            </For>
          </ul>
        </div>
      </div>
    </div>
  );
};
