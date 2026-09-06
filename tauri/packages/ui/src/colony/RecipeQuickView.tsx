/**
 * RecipeQuickView — modal preview before commit.
 *
 * Renders the backend-supplied recipe metadata + summary + input
 * count. The user picks USE (one-click deploy with required inputs)
 * or CUSTOMIZE (W2.A — opens TeamBuilder pre-loaded). Fork is a W2.B
 * affordance and stays disabled until that wave lands; the button is
 * present so users see the path before it activates.
 */
import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { Recipe } from "../dashboard/types";

export interface RecipeQuickViewProps {
  recipe: Recipe;
  isFavorite: boolean;
  onUse: () => void;
  onCustomize?: () => void;
  onFork?: () => void;
  onToggleFavorite: () => void;
  onCancel: () => void;
}

export const RecipeQuickView: Component<RecipeQuickViewProps> = (props) => {
  const summary = () => {
    const blueprint = props.recipe.blueprint as { summary?: string } | undefined;
    return blueprint?.summary ?? null;
  };
  // Derive each visibility tier from the single inputs array.
  const requiredInputs = () => props.recipe.inputs.filter((f) => f.visibility === "required");
  const optionalCount = () => props.recipe.inputs.filter((f) => f.visibility === "optional").length;
  const advancedCount = () => props.recipe.inputs.filter((f) => f.visibility === "advanced").length;
  return (
    <div class="mx-auto max-w-2xl rounded border-2 border-bark bg-soil-mid p-6">
      <div class="flex items-start justify-between">
        <div>
          <p class="colony-text-md font-bold text-text-primary">{props.recipe.name}</p>
          <p class="colony-text-xs mt-1 text-text-secondary">{props.recipe.description}</p>
        </div>
        <button
          type="button"
          class="colony-text-md hover:text-status-ok"
          aria-label={props.isFavorite ? "Remove from favorites" : "Add to favorites"}
          onClick={() => props.onToggleFavorite()}
        >
          {props.isFavorite ? "⭐" : "☆"}
        </button>
      </div>

      <Show when={summary()}>
        <div class="mt-4 rounded border border-bark bg-soil-deep p-3">
          <p class="colony-text-3xs text-text-dim">What this does</p>
          <p class="colony-text-xs mt-1 text-text-primary">{summary()}</p>
        </div>
      </Show>

      <div class="mt-4 grid grid-cols-2 gap-3 colony-text-2xs">
        <div>
          <p class="text-text-dim">Difficulty</p>
          <p class="text-text-primary">{props.recipe.difficulty}</p>
        </div>
        <div>
          <p class="text-text-dim">AI required</p>
          <p class="text-text-primary">{props.recipe.ai_required ? "Yes" : "Optional"}</p>
        </div>
        <Show when={props.recipe.connectors_used.length > 0}>
          <div class="col-span-2">
            <p class="text-text-dim">Connectors</p>
            <div class="mt-1 flex flex-wrap gap-1">
              <For each={props.recipe.connectors_used}>
                {(c) => (
                  <span class="colony-text-3xs rounded border border-bark bg-soil-deep px-2 py-1 text-text-primary">
                    {c.replace(/^connector-/, "")}
                  </span>
                )}
              </For>
            </div>
          </div>
        </Show>
      </div>

      <Show when={requiredInputs().length > 0}>
        <div class="mt-4">
          <p class="colony-text-3xs text-text-dim">You'll provide</p>
          <ul class="mt-1 space-y-1">
            <For each={requiredInputs()}>
              {(f) => (
                <li class="colony-text-2xs text-text-primary">
                  • {f.label}
                  <Show when={f.kind.kind === "secret"}>
                    <span class="colony-text-3xs ml-2 text-text-dim">(secret)</span>
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </div>
      </Show>

      <Show when={optionalCount() > 0 || advancedCount() > 0}>
        <p class="colony-text-3xs mt-3 text-text-dim">
          + {optionalCount()} optional, {advancedCount()} advanced (shown after you click USE)
        </p>
      </Show>

      <div class="mt-6 flex flex-wrap gap-2">
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-4 py-2"
          data-tone="ok"
          onClick={() => props.onUse()}
        >
          USE THIS
        </button>
        <Show when={props.onCustomize}>
          <button
            type="button"
            class="colony-command-btn colony-text-2xs px-4 py-2"
            onClick={() => props.onCustomize?.()}
          >
            CUSTOMIZE
          </button>
        </Show>
        <Show when={props.onFork}>
          <button
            type="button"
            class="colony-command-btn colony-text-2xs px-4 py-2"
            onClick={() => props.onFork?.()}
          >
            FORK
          </button>
        </Show>
        <div class="flex-1" />
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-4 py-2"
          onClick={() => props.onCancel()}
        >
          Back
        </button>
      </div>
    </div>
  );
};
