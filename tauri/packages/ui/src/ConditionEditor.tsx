import { For } from "solid-js";
import type { Component } from "solid-js";
import { useI18n } from "./i18n/context";

export interface ConditionDef {
  type: string;
  field?: string;
  value?: string;
  pattern?: string;
  start?: string;
  end?: string;
  days?: number[];
}

export interface ConditionEditorProps {
  conditions: ConditionDef[];
  /** Valid condition type names — must come from backend schema. */
  conditionTypes: string[];
  onChange: (conditions: ConditionDef[]) => void;
}

/**
 * Condition editor — add and configure rule conditions.
 *
 * Supports the core condition types from the Condition enum.
 * Each condition row is a <fieldset> with screen-reader-only <legend>.
 * Day-of-week buttons use aria-pressed for toggle state.
 */
export const ConditionEditor: Component<ConditionEditorProps> = (props) => {
  const { t } = useI18n();

  const DAY_KEYS = ["days.sun", "days.mon", "days.tue", "days.wed", "days.thu", "days.fri", "days.sat"];

  const addCondition = () => {
    const firstType = props.conditionTypes[0];
    if (!firstType) return;
    props.onChange([...props.conditions, { type: firstType, field: "", value: "" }]);
  };

  const removeCondition = (index: number) => {
    props.onChange(props.conditions.filter((_, i) => i !== index));
  };

  const updateCondition = (index: number, updates: Partial<ConditionDef>) => {
    const newConditions = [...props.conditions];
    const existing = newConditions[index];
    if (existing) {
      newConditions[index] = { ...existing, ...updates };
      props.onChange(newConditions);
    }
  };

  return (
    <div class="space-y-3">
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-300">{t("condition.title")}</label>
        <button
          class="rounded bg-gray-700 px-3 py-1 text-xs text-gray-200 hover:bg-gray-600"
          onClick={addCondition}
        >
          {t("condition.add")}
        </button>
      </div>

      <For each={props.conditions}>
        {(condition, index) => (
          <fieldset class="flex gap-2 rounded border border-gray-700 bg-gray-800/50 p-3">
            <legend class="sr-only">
              {t("condition.removeN", { n: String(index() + 1) })}
            </legend>

            <select
              aria-label={t("condition.conditionType")}
              class="rounded border border-gray-700 bg-gray-800 px-2 py-1 text-sm text-white"
              value={condition.type}
              onChange={(e) => updateCondition(index(), { type: e.currentTarget.value })}
            >
              <For each={props.conditionTypes}>
                {(type) => <option value={type}>{type}</option>}
              </For>
            </select>

            {(condition.type === "FieldEquals" || condition.type === "Contains" || condition.type === "Regex") && (
              <>
                <input
                  class="flex-1 rounded border border-gray-700 bg-gray-800 px-2 py-1 text-sm text-white"
                  aria-label={t("condition.field")}
                  placeholder={t("condition.field")}
                  value={condition.field ?? ""}
                  onInput={(e) => updateCondition(index(), { field: e.currentTarget.value })}
                />
                <input
                  class="flex-1 rounded border border-gray-700 bg-gray-800 px-2 py-1 text-sm text-white"
                  aria-label={condition.type === "Regex" ? t("condition.pattern") : t("condition.value")}
                  placeholder={condition.type === "Regex" ? t("condition.pattern") : t("condition.value")}
                  value={condition.type === "Regex" ? (condition.pattern ?? "") : (condition.value ?? "")}
                  onInput={(e) =>
                    updateCondition(index(), condition.type === "Regex" ? { pattern: e.currentTarget.value } : { value: e.currentTarget.value })
                  }
                />
              </>
            )}

            {condition.type === "TimeInRange" && (
              <>
                <input
                  class="rounded border border-gray-700 bg-gray-800 px-2 py-1 text-sm text-white"
                  aria-label={t("condition.startTime")}
                  placeholder={t("condition.timeFormat")}
                  value={condition.start ?? ""}
                  onInput={(e) => updateCondition(index(), { start: e.currentTarget.value })}
                />
                <span class="self-center text-gray-500">{t("common.to")}</span>
                <input
                  class="rounded border border-gray-700 bg-gray-800 px-2 py-1 text-sm text-white"
                  aria-label={t("condition.endTime")}
                  placeholder={t("condition.timeFormat")}
                  value={condition.end ?? ""}
                  onInput={(e) => updateCondition(index(), { end: e.currentTarget.value })}
                />
              </>
            )}

            {condition.type === "DayOfWeek" && (
              <div role="group" aria-label={t("condition.dayGroup")} class="flex flex-wrap gap-1">
                <For each={DAY_KEYS}>
                  {(dayKey, dayIndex) => {
                    const isSelected = () => (condition.days ?? []).includes(dayIndex());
                    return (
                      <button
                        aria-pressed={isSelected()}
                        class={`rounded px-2 py-1 text-xs ${
                          isSelected()
                            ? "bg-blue-600 text-white"
                            : "bg-gray-700 text-gray-300 hover:bg-gray-600"
                        }`}
                        onClick={() => {
                          const current = condition.days ?? [];
                          const updated = isSelected()
                            ? current.filter((d) => d !== dayIndex())
                            : [...current, dayIndex()];
                          updateCondition(index(), { days: updated });
                        }}
                      >
                        {t(dayKey)}
                      </button>
                    );
                  }}
                </For>
              </div>
            )}

            <button
              class="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-900/30"
              aria-label={t("condition.removeN", { n: String(index() + 1) })}
              onClick={() => removeCondition(index())}
            >
              {t("common.remove")}
            </button>
          </fieldset>
        )}
      </For>

      {props.conditions.length === 0 && (
        <p role="status" class="text-sm text-gray-500">{t("condition.noConditions")}</p>
      )}
    </div>
  );
};
