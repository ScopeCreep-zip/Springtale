import { For, Show } from "solid-js";
import type { Component } from "solid-js";
import type { FormationSummary, MomentumTier } from "@springtale/types";
import { useI18n } from "./i18n/context";

export interface FormationCardProps {
  formation: FormationSummary;
  selected: boolean;
  onSelect: () => void;
}

/**
 * Formation card — renders a formation as an RTS unit on the canvas.
 *
 * Shows: formation name (intent), member count, momentum tier,
 * viability status. Click to select and show details in command panel.
 *
 * Per COOPERATION.pdf: formations are peer groups, not hierarchies.
 * The card represents the formation as a single unit the user
 * can inspect, command, and monitor.
 */
export const FormationCard: Component<FormationCardProps> = (props) => {
  const { t } = useI18n();

  const tierColor = (tier: MomentumTier): string => {
    switch (tier) {
      case "Cold": return "border-gray-600 bg-gray-900";
      case "Warming": return "border-blue-600/50 bg-blue-900/20";
      case "Hot": return "border-orange-500/50 bg-orange-900/20";
      case "Fever": return "border-green-500/50 bg-green-900/20";
      default: return "border-gray-600 bg-gray-900";
    }
  };

  const tierDot = (tier: MomentumTier): string => {
    switch (tier) {
      case "Cold": return "bg-gray-500";
      case "Warming": return "bg-blue-400";
      case "Hot": return "bg-orange-400";
      case "Fever": return "bg-green-400 motion-safe:animate-pulse";
      default: return "bg-gray-500";
    }
  };

  return (
    <button
      onClick={() => props.onSelect()}
      class={`rounded border p-3 text-start text-xs transition-colors ${
        props.selected
          ? "border-blue-500 bg-blue-900/30"
          : tierColor(props.formation.momentum_tier)
      } ${!props.formation.is_viable ? "opacity-50" : ""}`}
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <span
            class={`inline-block h-2 w-2 rounded-full ${tierDot(props.formation.momentum_tier)}`}
            aria-hidden="true"
          />
          <span class="font-medium text-white">{props.formation.intent}</span>
        </div>
        <span class="text-gray-500">{props.formation.momentum_tier}</span>
      </div>
      <div class="mt-1 flex items-center gap-3 text-gray-500">
        <span>{props.formation.operational_count}/{props.formation.member_count} agents</span>
        <Show when={!props.formation.is_viable}>
          <span class="text-red-400">non-viable</span>
        </Show>
      </div>
    </button>
  );
};
