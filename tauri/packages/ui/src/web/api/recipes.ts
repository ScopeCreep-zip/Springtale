/**
 * W1.B — HTTP wrappers for the recipe library.
 *
 * Mirrors the desktop IPC surface — both consume the same server-side
 * operations (see `apps/springtaled/src/api/recipes.rs`).
 */
import type {
  PreflightReport,
  PreviewReport,
  Recipe,
  RecipeApplyReport,
  RecipeCategory,
  RecipeFilter,
  RecipeInputs,
  RecipePieceSummary,
} from "../../dashboard/types";

import { get, getBaseUrl, getToken, post } from "./client";

/** Build the query string for `GET /recipes`. Filter fields are
 *  serialised the same way the springtaled handler decodes them
 *  (`RecipeListQuery` in `api/recipes.rs`). */
function recipeFilterParams(filter?: RecipeFilter): string {
  if (!filter) return "";
  const params = new URLSearchParams();
  if (filter.query) params.set("query", filter.query);
  if (filter.category) params.set("category", filter.category);
  if (filter.tags && filter.tags.length > 0) params.set("tags", filter.tags.join(","));
  if (filter.sources && filter.sources.length > 0) params.set("sources", filter.sources.join(","));
  if (filter.favorites_only) params.set("favorites_only", "true");
  if (typeof filter.limit === "number") params.set("limit", String(filter.limit));
  if (filter.sort) params.set("sort", filter.sort);
  const s = params.toString();
  return s ? `?${s}` : "";
}

export async function listRecipes(filter?: RecipeFilter): Promise<Recipe[]> {
  return get<Recipe[]>(`/recipes${recipeFilterParams(filter)}`);
}

export async function getRecipe(id: string): Promise<Recipe | null> {
  try {
    return await get<Recipe>(`/recipes/${encodeURIComponent(id)}`);
  } catch (err) {
    // 404 surfaces as the generic "API error" string from client.ts;
    // surface it as null so consumers can decide what to render.
    if (String(err).includes("404")) return null;
    throw err;
  }
}

export async function listRecipeCategories(): Promise<RecipeCategory[]> {
  return get<RecipeCategory[]>(`/recipes/categories`);
}

export async function toggleRecipeFavorite(recipeId: string): Promise<boolean> {
  const resp = await post<{ recipe_id: string; now_favorite: boolean }>(
    `/recipes/${encodeURIComponent(recipeId)}/favorite`,
  );
  return resp.now_favorite;
}

export async function recordRecipeRecent(recipeId: string): Promise<void> {
  // 204 No Content — call the client without trying to parse a body.
  const response = await fetch(`${getBaseUrl()}/recipes/${encodeURIComponent(recipeId)}/recent`, {
    method: "POST",
    headers: { Authorization: `Bearer ${getToken()}` },
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }
}

export async function applyRecipe(
  recipeId: string,
  inputs: RecipeInputs,
): Promise<RecipeApplyReport> {
  return post<RecipeApplyReport>(`/recipes/${encodeURIComponent(recipeId)}/apply`, inputs);
}

export async function preflightRecipe(
  recipeId: string,
  inputs: RecipeInputs,
): Promise<PreflightReport> {
  return post<PreflightReport>(`/recipes/${encodeURIComponent(recipeId)}/preflight`, inputs);
}

export async function previewRecipe(
  recipeId: string,
  inputs: RecipeInputs,
): Promise<PreviewReport> {
  return post<PreviewReport>(`/recipes/${encodeURIComponent(recipeId)}/preview`, inputs);
}

export async function listRecipePieces(recipeId: string): Promise<RecipePieceSummary[]> {
  return get<RecipePieceSummary[]>(`/recipes/${encodeURIComponent(recipeId)}/pieces`);
}

// ── W2.B Recipe authoring ────────────────────────────────────

export async function saveUserRecipe(recipe: Recipe): Promise<Recipe> {
  return post<Recipe>(`/recipes/user`, recipe);
}

export async function forkRecipe(recipeId: string, newName: string): Promise<Recipe> {
  return post<Recipe>(`/recipes/${encodeURIComponent(recipeId)}/fork`, { new_name: newName });
}

export async function deleteUserRecipe(recipeId: string): Promise<boolean> {
  const response = await fetch(`${getBaseUrl()}/recipes/user/${encodeURIComponent(recipeId)}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${getToken()}` },
  });
  if (response.status === 404) return false;
  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }
  return true;
}

export async function exportRecipeToml(recipeId: string): Promise<string> {
  const response = await fetch(`${getBaseUrl()}/recipes/${encodeURIComponent(recipeId)}/export`, {
    headers: { Authorization: `Bearer ${getToken()}` },
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }
  return response.text();
}

export async function importRecipeToml(toml: string): Promise<Recipe> {
  const response = await fetch(`${getBaseUrl()}/recipes/import`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${getToken()}`,
      "Content-Type": "text/plain",
    },
    body: toml,
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<Recipe>;
}

export async function renderRecipeToml(recipeId: string, inputs: RecipeInputs): Promise<string> {
  // The render endpoint returns a plain string body.
  const response = await fetch(`${getBaseUrl()}/recipes/${encodeURIComponent(recipeId)}/render`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${getToken()}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(inputs),
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }
  return response.text();
}
