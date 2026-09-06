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
  // HTML5 disallows nested interactive content: a `<button>` inside a
  // `<button>` is a parse error and the parser auto-closes the outer
  // one, scattering its remaining children as siblings. That breaks the
  // SolidJS dom-expressions template walker (it asserts the templated
  // child tree and dereferences `_el$N.nextSibling`, which becomes
  // `null` after the auto-close — see the recipe-stack mount crash we
  // chased through Phase B). We keep the favourite toggle as a SIBLING
  // of the card button, positioned absolutely, so both can be real
  // `<button>` elements.
  return (
    <article class="relative h-full">
      <button
        type="button"
        class="colony-command-btn flex h-full min-h-[160px] w-full flex-col items-start gap-2 p-4 text-left transition"
        onClick={() => props.onClick()}
      >
        <div class="colony-text-xl">
          {props.recipe.icon_id ? labelGlyph(props.recipe.icon_id) : "📦"}
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
      </button>
      <button
        type="button"
        class="colony-text-sm absolute right-4 top-4 hover:text-status-ok"
        aria-label={props.isFavorite ? "Remove from favorites" : "Add to favorites"}
        onClick={(e) => {
          e.stopPropagation();
          props.onToggleFavorite();
        }}
      >
        {props.isFavorite ? "⭐" : "☆"}
      </button>
    </article>
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
