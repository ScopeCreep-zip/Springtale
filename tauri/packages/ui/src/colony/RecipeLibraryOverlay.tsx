/**
 * RecipeLibraryOverlay — W1.B click-and-play recipe surface.
 *
 * Renders ONLY the library grid. The recipe quick-view modal is
 * mounted separately by `App.tsx` via the `recipeQuickView` signal —
 * this component never switches into a quick-view in place. Lifting
 * that state out keeps the JSX structurally flat (sidebar + grid)
 * and mirrors the proven `MemberPickerOverlay` shape: a single
 * outer `<div>` with several sibling `<Show>` blocks reading the
 * resource state directly. Nested view-switching inside this
 * component was the source of repeated `_el$N.nextSibling = null`
 * crashes during the loading → resolved Suspense flip.
 *
 * Architecture: all filtering / sorting / favorites / recent are
 * decided server-side in `springtale-runtime::operations::recipes`.
 * The frontend dispatches a `RecipeFilter` and renders the
 * response. Zero frontend business logic; the empty-state copy
 * strings are pure presentation.
 */

import type { Component } from "solid-js";
import { createMemo, createResource, createSignal, For, onMount, Show } from "solid-js";
import { useDashboard } from "../dashboard/context";
import type { Recipe, RecipeCategory, RecipeFilter } from "../dashboard/types";
import { RecipeCard } from "./RecipeCard";

/** Visual variant — applied as a hard server-side filter so the
 *  library opens scoped to the user's intent from ModeSelectOverlay. */
export type RecipeLibraryVariant = "bot" | "team" | "all";

export interface RecipeLibraryOverlayProps {
  variant?: RecipeLibraryVariant;
  /** Set of recipe ids the user has favorited. Owned by App.tsx so
   *  RecipeQuickView sees the same set. */
  favorites: Set<string>;
  /** Click on a recipe card — App.tsx mounts the quick-view modal. */
  onSelectRecipe: (recipe: Recipe) => void;
  /** Toggle the favorite for a recipe (heart icon on a card). */
  onToggleFavorite: (recipe: Recipe) => void;
  /** Power-user escape hatch — start from a blank slate. */
  onBuildFromScratch: () => void;
  /** Close the library. */
  onCancel: () => void;
}

/** Value-equality on `RecipeFilter` so the `createMemo` source
 *  identity stays stable when filter content is stable. Without
 *  this the memo returns a fresh object on every reactive tick and
 *  `createResource` refetches needlessly. */
function shallowFilterEqual(a: RecipeFilter, b: RecipeFilter): boolean {
  return (
    a.query === b.query &&
    a.category === b.category &&
    a.favorites_only === b.favorites_only &&
    a.sort === b.sort &&
    arrayEq(a.tags, b.tags) &&
    arrayEq(a.sources, b.sources)
  );
}
function arrayEq<T>(a: T[] | undefined, b: T[] | undefined): boolean {
  if (a === b) return true;
  const al = a?.length ?? 0;
  const bl = b?.length ?? 0;
  if (al !== bl) return false;
  for (let i = 0; i < al; i++) if (a?.[i] !== b?.[i]) return false;
  return true;
}

type SidebarBucket =
  | { kind: "all" }
  | { kind: "category"; value: RecipeCategory }
  | { kind: "favorites" }
  | { kind: "recent" }
  | { kind: "user" };

export const RecipeLibraryOverlay: Component<RecipeLibraryOverlayProps> = (props) => {
  const db = useDashboard();
  const [bucket, setBucket] = createSignal<SidebarBucket>({ kind: "all" });
  const [query, setQuery] = createSignal("");

  const [categories] = createResource(() => db.provider.listRecipeCategories());

  const filter = createMemo<RecipeFilter>(
    () => {
      const f: RecipeFilter = {};
      const b = bucket();
      if (b.kind === "category") f.category = b.value;
      if (b.kind === "favorites") f.favorites_only = true;
      if (b.kind === "recent") f.sort = "recent";
      if (b.kind === "user") f.sources = ["user"];
      if (query().trim().length > 0) f.query = query().trim();
      return f;
    },
    {} as RecipeFilter,
    { equals: shallowFilterEqual },
  );

  const [recipes] = createResource(filter, (f) => db.provider.listRecipes(f));

  // Refresh-once on mount is no longer needed for favorites — App.tsx
  // owns favorites and seeds them before mounting the library.
  onMount(() => {});

  return (
    <div class="mx-4 flex h-[85vh] w-full max-w-6xl rounded border-2 border-bark bg-soil-mid">
      {/* ── Sidebar ── */}
      <aside class="w-44 shrink-0 overflow-y-auto border-r-2 border-bark p-3">
        <SidebarItem
          label="All recipes"
          active={bucket().kind === "all"}
          onClick={() => setBucket({ kind: "all" })}
        />
        <div class="colony-text-3xs mt-3 text-text-dim">CATEGORIES</div>
        <For each={categories() ?? []}>
          {(cat) => (
            <SidebarItem
              label={categoryLabel(cat)}
              active={
                bucket().kind === "category" &&
                (bucket() as { value: RecipeCategory }).value === cat
              }
              onClick={() => setBucket({ kind: "category", value: cat })}
            />
          )}
        </For>
        <div class="colony-text-3xs mt-3 text-text-dim">COLLECTIONS</div>
        <SidebarItem
          label="⭐ Favorites"
          active={bucket().kind === "favorites"}
          onClick={() => setBucket({ kind: "favorites" })}
        />
        <SidebarItem
          label="🕒 Recent"
          active={bucket().kind === "recent"}
          onClick={() => setBucket({ kind: "recent" })}
        />
        <SidebarItem
          label="👤 My recipes"
          active={bucket().kind === "user"}
          onClick={() => setBucket({ kind: "user" })}
        />
        <button
          type="button"
          class="colony-command-btn colony-text-2xs mt-4 w-full px-2 py-2"
          onClick={() => props.onBuildFromScratch()}
        >
          ➕ Build from scratch
        </button>
      </aside>

      {/* ── Main pane ── */}
      <section class="flex flex-1 flex-col overflow-hidden">
        <header class="border-b-2 border-bark p-3">
          <div class="flex items-center gap-2">
            <p class="colony-text-md font-bold text-text-primary">{bucketTitle(bucket())}</p>
            <div class="flex-1" />
            <button
              type="button"
              class="colony-command-btn colony-text-2xs px-3 py-1"
              onClick={() => props.onCancel()}
            >
              Close
            </button>
          </div>
          <input
            class="colony-text-xs mt-3 w-full border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
            placeholder="Search recipes or describe what you want..."
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </header>

        <div class="flex-1 overflow-y-auto p-3">
          <Show when={recipes.loading}>
            <p class="colony-text-xs text-text-dim">Loading recipes…</p>
          </Show>

          <Show when={recipes.error}>
            <div class="rounded border border-status-error bg-status-error/10 p-3">
              <p class="colony-text-2xs text-status-error">
                Couldn't load recipes: {String(recipes.error)}
              </p>
            </div>
          </Show>

          <Show when={!recipes.loading && !recipes.error && recipes() && recipes()?.length === 0}>
            <div class="flex h-full flex-col items-center justify-center gap-3 text-center">
              <p class="colony-text-xs text-text-dim">{emptyMessage(bucket(), query())}</p>
              <button
                type="button"
                class="colony-command-btn colony-text-2xs px-4 py-2"
                onClick={() => props.onBuildFromScratch()}
              >
                Build from scratch
              </button>
            </div>
          </Show>

          <Show when={(recipes()?.length ?? 0) > 0}>
            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
              <For each={recipes()}>
                {(r) => (
                  <RecipeCard
                    recipe={r}
                    isFavorite={props.favorites.has(r.id)}
                    onClick={() => props.onSelectRecipe(r)}
                    onToggleFavorite={() => props.onToggleFavorite(r)}
                  />
                )}
              </For>
            </div>
          </Show>
        </div>
      </section>
    </div>
  );
};

interface SidebarItemProps {
  label: string;
  active: boolean;
  onClick: () => void;
}

const SidebarItem: Component<SidebarItemProps> = (props) => (
  <button
    type="button"
    class="colony-text-xs w-full rounded px-2 py-1 text-left text-text-primary transition hover:bg-soil-deep"
    classList={{ "bg-soil-deep border-l-2 border-accent": props.active }}
    onClick={() => props.onClick()}
  >
    {props.label}
  </button>
);

function categoryLabel(c: RecipeCategory): string {
  switch (c) {
    case "messaging":
      return "Messaging";
    case "coding":
      return "Coding";
    case "web":
      return "Web";
    case "ai_assistant":
      return "AI assistants";
    case "daily":
      return "Daily tasks";
    case "safety_privacy":
      return "Safety & privacy";
    case "custom":
      return "Custom";
  }
}

function bucketTitle(b: SidebarBucket): string {
  switch (b.kind) {
    case "all":
      return "All recipes";
    case "category":
      return categoryLabel(b.value);
    case "favorites":
      return "Favorites";
    case "recent":
      return "Recent";
    case "user":
      return "My recipes";
  }
}

function emptyMessage(bucket: SidebarBucket, query: string): string {
  if (query.length > 0) return `No recipes match "${query}".`;
  if (bucket.kind === "user")
    return 'You haven\'t saved any recipes yet. Build a bot, then "Save as recipe."';
  return "No recipes match. Try a different filter.";
}
