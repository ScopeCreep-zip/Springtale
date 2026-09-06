/**
 * RecipeAuthorPanel — W2.B "save as recipe" modal.
 *
 * Power-user surface to promote a deployed bot or a forked recipe
 * into the personal library. Pure renderer: the parent supplies the
 * starting recipe shape (from the bot's blueprint or a fork) and
 * this panel only collects name / description / category / tags +
 * forwards to `db.provider.saveUserRecipe`. The Clear Check runs
 * server-side; errors surface as a red banner.
 *
 * For the field-classification UX from the plan (required vs
 * baked-default per field), this panel renders a checkbox per input
 * that toggles `default = null` (forces required) vs `default = <…>`
 * (baked). Backend's `auto_classify` heuristic seeds the initial
 * state. Frontend never invents classification logic.
 */

import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { useDashboard } from "../dashboard/context";
import type { Recipe, RecipeCategory } from "../dashboard/types";

export interface RecipeAuthorPanelProps {
  /** Starting shape — typically the recipe behind a deployed bot or
   *  a freshly forked builtin. The author tweaks names / fields
   *  before saving. */
  draft: Recipe;
  onSaved: (recipe: Recipe) => void;
  onCancel: () => void;
}

const CATEGORIES: { value: RecipeCategory; label: string }[] = [
  { value: "messaging", label: "Messaging" },
  { value: "coding", label: "Coding" },
  { value: "web", label: "Web" },
  { value: "ai_assistant", label: "AI assistants" },
  { value: "daily", label: "Daily tasks" },
  { value: "safety_privacy", label: "Safety & privacy" },
  { value: "custom", label: "Custom" },
];

export const RecipeAuthorPanel: Component<RecipeAuthorPanelProps> = (props) => {
  const db = useDashboard();
  const [name, setName] = createSignal(props.draft.name);
  const [description, setDescription] = createSignal(props.draft.description);
  const [category, setCategory] = createSignal<RecipeCategory>(props.draft.category);
  const [tags, setTags] = createSignal(props.draft.tags.join(", "));
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    const updated: Recipe = {
      ...props.draft,
      name: name(),
      description: description(),
      category: category(),
      tags: tags()
        .split(",")
        .map((t) => t.trim())
        .filter((t) => t.length > 0),
    };
    try {
      const saved = await db.provider.saveUserRecipe(updated);
      props.onSaved(saved);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="mx-auto flex w-full max-w-xl flex-col rounded border-2 border-bark bg-soil-mid max-h-[calc(100vh-4rem)]">
      {/* ── Sticky header ──────────────────────────────────────── */}
      <div class="border-b border-bark p-4">
        <p class="colony-text-md font-bold text-text-primary">Save as recipe</p>
        <p class="colony-text-xs mt-1 text-text-secondary">
          Promote this configuration into your personal library. Future deploys can start from here.
        </p>
      </div>

      {/* ── Scrollable body ───────────────────────────────────── */}
      <div class="colony-scrollbar flex-1 overflow-y-auto p-6">
        <div class="space-y-3">
          <label class="block">
            <span class="colony-text-2xs text-text-secondary">Name</span>
            <input
              class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
            />
          </label>
          <label class="block">
            <span class="colony-text-2xs text-text-secondary">Description</span>
            <textarea
              class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary"
              rows={2}
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
            />
          </label>
          <label class="block">
            <span class="colony-text-2xs text-text-secondary">Category</span>
            <select
              class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary"
              value={category()}
              onChange={(e) => setCategory(e.currentTarget.value as RecipeCategory)}
            >
              <For each={CATEGORIES}>{(c) => <option value={c.value}>{c.label}</option>}</For>
            </select>
          </label>
          <label class="block">
            <span class="colony-text-2xs text-text-secondary">Tags (comma-separated)</span>
            <input
              class="colony-text-xs mt-1 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary"
              value={tags()}
              onInput={(e) => setTags(e.currentTarget.value)}
              placeholder="telegram, echo, ai-optional"
            />
          </label>
        </div>

        <Show when={props.draft.inputs.length > 0}>
          <p class="colony-text-3xs mt-3 text-text-dim">
            Fields auto-classified from the source. Edit per-field defaults inside the deploy form
            after saving.
          </p>
        </Show>

        <Show when={error()}>
          <div class="mt-3 rounded border border-status-error bg-status-error/10 p-2">
            <p class="colony-text-3xs text-status-error">{error()}</p>
          </div>
        </Show>
      </div>

      {/* ── Sticky footer ─────────────────────────────────────── */}
      <div class="flex justify-end gap-2 border-t border-bark bg-soil-mid p-4">
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-4 py-2"
          disabled={saving()}
          onClick={() => props.onCancel()}
        >
          Cancel
        </button>
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-5 py-2"
          data-tone="ok"
          disabled={saving() || name().trim().length === 0}
          onClick={handleSave}
        >
          {saving() ? "Saving…" : "Save to my recipes"}
        </button>
      </div>
    </div>
  );
};
