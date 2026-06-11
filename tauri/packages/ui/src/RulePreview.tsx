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

  // `aria-labelledby` ties the section to its heading so screen
  // readers announce the region correctly; `<section>` is the right
  // semantic landmark.
  return (
    <section aria-labelledby="rule-preview-heading">
      <h3 id="rule-preview-heading" class="block colony-text-xs font-medium text-text-secondary">
        {t("preview.label")}
      </h3>
      <pre class="colony-textarea mt-1 max-h-64 overflow-auto rounded border border-bark bg-soil-deep p-3 colony-text-2xs text-text-secondary">
        {props.toml || t("preview.placeholder")}
      </pre>
    </section>
  );
};
