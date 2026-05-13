/**
 * RecipeCard — the visual unit in the W1.B library grid.
 *
 * Pure renderer: every value comes from the `Recipe` the backend
 * returned. Click bubbles up so the parent decides whether to open
 * the quick-view, fork, customise, etc.
 */
import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { Difficulty, Recipe, RecipeSource } from "../dashboard/types";

export interface RecipeCardProps {
  recipe: Recipe;
  isFavorite: boolean;
  onClick: () => void;
  onToggleFavorite: () => void;
}

const STAR_FILLED = "★";
const STAR_EMPTY = "☆";

/** Map server difficulty to ★ count + label, both rendered by the
 *  card. Keep this in lock-step with the backend `Difficulty` enum. */
function difficultyStars(d: Difficulty): { stars: string; label: string } {
  switch (d) {
    case "quick":
      return { stars: `${STAR_FILLED}${STAR_EMPTY}${STAR_EMPTY}`, label: "Quick" };
    case "standard":
      return { stars: `${STAR_FILLED}${STAR_FILLED}${STAR_EMPTY}`, label: "Standard" };
    case "power":
      return { stars: `${STAR_FILLED}${STAR_FILLED}${STAR_FILLED}`, label: "Power" };
  }
}

function sourceBadge(s: RecipeSource): string | null {
  switch (s.kind) {
    case "builtin":
      return null;
    case "user":
      return "by you";
    case "community":
      return "community";
  }
}

export const RecipeCard: Component<RecipeCardProps> = (props) => {
  const diff = () => difficultyStars(props.recipe.difficulty);
  const badge = () => sourceBadge(props.recipe.source);
  // Outer wrapper is a `<div role="button">`, NOT a real `<button>`.
  // The card contains an interactive favorite-toggle child button —
  // and per HTML5 parsing rules, a `<button>` nested inside a
  // `<button>` is a parse error: the parser implicitly CLOSES the
  // outer button when it encounters the inner one, scattering the
  // remaining children as siblings of the outer. That collapsed DOM
  // shape diverges from the static-template shape the dom-expressions
  // compiler walks via `_el$N.firstChild` / `.nextSibling`, so the
  // walker dereferences `_el$5.nextSibling = null` at mount time
  // (the symptom we kept chasing through the recipe stack). Using a
  // div + `role="button"` + keyboard handler keeps the inner
  // favorite `<button>` valid and the DOM tree matches the template.
  const handleKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      props.onClick();
    }
  };
  return (
    <div
      class="colony-command-btn flex h-full flex-col items-start gap-2 p-4 text-left transition"
      style={{ "min-height": "160px", cursor: "pointer" }}
      role="button"
      tabindex="0"
      onClick={() => props.onClick()}
      onKeyDown={handleKey}
    >
      <div class="flex w-full items-center justify-between">
        <div class="colony-text-xl">{props.recipe.icon_id ? labelGlyph(props.recipe.icon_id) : "📦"}</div>
        <button
          class="colony-text-sm hover:text-status-ok"
          aria-label={props.isFavorite ? "Remove from favorites" : "Add to favorites"}
          onClick={(e) => {
            e.stopPropagation();
            props.onToggleFavorite();
          }}
        >
          {props.isFavorite ? "⭐" : "☆"}
        </button>
      </div>

      <div class="colony-text-sm font-bold text-text-primary">{props.recipe.name}</div>
      <div class="colony-text-2xs flex-1 text-text-secondary line-clamp-2">
        {props.recipe.description}
      </div>

      <div class="flex w-full items-center justify-between">
        <div class="colony-text-3xs text-text-dim" title={diff().label}>
          {diff().stars}
        </div>
        <Show when={badge()}>
          <div class="colony-text-3xs text-text-dim">[{badge()}]</div>
        </Show>
      </div>

      <Show when={props.recipe.connectors_used.length > 0}>
        <div class="flex flex-wrap gap-1">
          <For each={props.recipe.connectors_used}>
            {(c) => (
              <span class="colony-text-3xs rounded border border-bark bg-soil-deep px-1 py-px text-text-dim">
                {c.replace(/^connector-/, "")}
              </span>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

/** Tiny client-side mapping from `icon_id` keyword to display glyph.
 *  Backend owns the keyword; the visual rendering is the frontend's
 *  thin choice. */
function labelGlyph(id: string): string {
  switch (id) {
    case "telegram":
      return "📨";
    case "github":
      return "🐙";
    case "robot":
      return "🤖";
    case "newspaper":
      return "📰";
    case "wrench":
      return "🔧";
    case "globe":
      return "🌐";
    case "alarm":
      return "⏰";
    case "shield":
      return "🛡";
    default:
      return "📦";
  }
}
