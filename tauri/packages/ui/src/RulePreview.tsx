import type { Component } from "solid-js";
import { useI18n } from "./i18n/context";

export interface RulePreviewProps {
  toml: string;
}

/**
 * Rule preview — shows generated TOML for the rule.
 *
 * Read-only view of what will be saved. Users can copy-paste
 * this into a .toml file if they prefer manual editing.
 *
 * role="region" makes this a navigable landmark for screen readers.
 */
export const RulePreview: Component<RulePreviewProps> = (props) => {
  const { t } = useI18n();

  return (
    <div role="region" aria-label={t("preview.label")}>
      <label class="block text-sm font-medium text-gray-300">
        {t("preview.label")}
      </label>
      <pre class="mt-1 max-h-64 overflow-auto rounded border border-gray-700 bg-gray-900 p-3 text-sm text-gray-300">
        {props.toml || t("preview.placeholder")}
      </pre>
    </div>
  );
};
