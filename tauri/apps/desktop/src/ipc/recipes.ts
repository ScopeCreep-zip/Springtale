/**
 * W1.B — Tauri IPC wrappers for the recipe library.
 *
 * Thin pass-through. All filtering / sorting / persistence happens
 * server-side; this module only translates between the SolidJS
 * provider surface and the `commands::recipes` Tauri commands.
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
} from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";

export async function listRecipes(filter?: RecipeFilter): Promise<Recipe[]> {
  return invoke<Recipe[]>("list_recipes", { filter: filter ?? null });
}

export async function getRecipe(id: string): Promise<Recipe | null> {
  return invoke<Recipe | null>("get_recipe", { id });
}

export async function listRecipeCategories(): Promise<RecipeCategory[]> {
  return invoke<RecipeCategory[]>("list_recipe_categories");
}

export async function toggleRecipeFavorite(recipeId: string): Promise<boolean> {
  return invoke<boolean>("toggle_recipe_favorite", { recipeId });
}

export async function recordRecipeRecent(recipeId: string): Promise<void> {
  return invoke("record_recipe_recent", { recipeId });
}

export async function applyRecipe(
  recipeId: string,
  inputs: RecipeInputs,
): Promise<RecipeApplyReport> {
  return invoke<RecipeApplyReport>("apply_recipe", { recipeId, inputs });
}

export async function renderRecipeToml(recipeId: string, inputs: RecipeInputs): Promise<string> {
  return invoke<string>("render_recipe_toml", { recipeId, inputs });
}

export async function preflightRecipe(
  recipeId: string,
  inputs: RecipeInputs,
): Promise<PreflightReport> {
  return invoke<PreflightReport>("preflight_recipe", { recipeId, inputs });
}

export async function previewRecipe(
  recipeId: string,
  inputs: RecipeInputs,
): Promise<PreviewReport> {
  return invoke<PreviewReport>("preview_recipe", { recipeId, inputs });
}

export async function listRecipePieces(recipeId: string): Promise<RecipePieceSummary[]> {
  return invoke<RecipePieceSummary[]>("list_recipe_pieces", { recipeId });
}

// ── W2.B Recipe authoring ────────────────────────────────────

export async function saveUserRecipe(recipe: Recipe): Promise<Recipe> {
  return invoke<Recipe>("save_user_recipe", { recipe });
}

export async function forkRecipe(recipeId: string, newName: string): Promise<Recipe> {
  return invoke<Recipe>("fork_recipe", { recipeId, newName });
}

export async function deleteUserRecipe(recipeId: string): Promise<boolean> {
  return invoke<boolean>("delete_user_recipe", { recipeId });
}

export async function exportRecipeToml(recipeId: string): Promise<string> {
  return invoke<string>("export_recipe_toml", { recipeId });
}

export async function importRecipeToml(tomlPayload: string): Promise<Recipe> {
  return invoke<Recipe>("import_recipe_toml", { tomlPayload });
}
