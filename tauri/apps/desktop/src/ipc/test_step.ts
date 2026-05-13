/**
 * Typed IPC wrapper for Phase C Test This Step.
 *
 * Backend runs the chain through `step_index` in DryRun mode and
 * returns the targeted step's StepOutput, plus every upstream
 * step it ran along the way.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  RecipeInputs,
  TestStepReport,
} from "@springtale/ui/dashboard/types";

export async function testRecipeStep(
  recipeId: string,
  inputs: RecipeInputs,
  ruleIndex: number,
  stepIndex: number,
): Promise<TestStepReport> {
  return invoke<TestStepReport>("test_recipe_step", {
    recipeId,
    inputs,
    ruleIndex,
    stepIndex,
  });
}
