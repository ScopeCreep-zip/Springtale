import type { Component } from "solid-js";
import { useI18n } from "./i18n/context";

export interface SwarmInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
  members: string[];
}

export interface SwarmCardProps {
  swarm: SwarmInfo;
  selected: boolean;
  onSelect: () => void;
}

/**
 * Swarm card — renders a swarm as a unit on the command canvas.
 *
 * Each swarm is a colony of springtails working in a substrate.
 * The card shows: name, intent, status, member count.
 * Click to select and show controls in the command panel.
 */
export const SwarmCard: Component<SwarmCardProps> = (props) => {
  const statusColor = (): string => {
    switch (props.swarm.status) {
      case "active": return "bg-green-500";
      case "paused": return "bg-yellow-500";
      case "draft": return "bg-gray-500";
      case "dissolved": return "bg-gray-700";
      default: return "bg-gray-500";
    }
  };

  const borderClass = (): string => {
    if (props.selected) return "border-blue-500 bg-blue-900/20";
    switch (props.swarm.status) {
      case "active": return "border-green-800/50 bg-gray-900 hover:border-green-700/50";
      case "paused": return "border-yellow-800/50 bg-gray-900 hover:border-yellow-700/50";
      default: return "border-gray-800 bg-gray-900 hover:border-gray-700";
    }
  };

  return (
    <button
      onClick={() => props.onSelect()}
      class={`rounded border p-3 text-start text-xs transition-colors ${borderClass()}`}
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <span class={`inline-block h-2 w-2 rounded-full ${statusColor()}`} aria-hidden="true" />
          <span class="font-medium text-white">{props.swarm.name}</span>
        </div>
      </div>
      <div class="mt-1 flex items-center gap-3 text-gray-500">
        <span>{props.swarm.intent}</span>
        <span>·</span>
        <span>{props.swarm.member_count} agents</span>
      </div>
    </button>
  );
};
