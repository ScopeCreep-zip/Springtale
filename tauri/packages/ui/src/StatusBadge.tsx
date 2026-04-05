import type { Component } from "solid-js";
import { useI18n } from "./i18n/context";

export interface StatusBadgeProps {
  status: "enabled" | "disabled" | "error" | "unknown";
  label?: string;
}

/**
 * Status indicator badge — enabled (green), disabled (gray), error (red).
 *
 * role="status" for screen readers. aria-label provides
 * translated status text even when visual label is abbreviated.
 */
export const StatusBadge: Component<StatusBadgeProps> = (props) => {
  const { t } = useI18n();

  const colorClass = () => {
    switch (props.status) {
      case "enabled":
        return "bg-green-500/20 text-green-400 border-green-500/30";
      case "disabled":
        return "bg-gray-500/20 text-gray-400 border-gray-500/30";
      case "error":
        return "bg-red-500/20 text-red-400 border-red-500/30";
      default:
        return "bg-gray-500/20 text-gray-400 border-gray-500/30";
    }
  };

  return (
    <span
      role="status"
      aria-label={props.label ?? t("status." + props.status)}
      class={`inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium ${colorClass()}`}
    >
      {props.label ?? t("status." + props.status)}
    </span>
  );
};
