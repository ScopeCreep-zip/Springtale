/**
 * ModeSelectOverlay — first-action hub when the user wants to create
 * something in the colony.
 *
 * Three big cards (Nintendo-style mode-select):
 *   - Make a Bot      → single-agent recipe library
 *   - Make a Team     → team recipe library / TeamBuilder
 *   - Add to a Team   → pick existing team, then add a bot
 *
 * Per `feedback_bimbo_mode` and `feedback_multi_path_oobe`: this is the
 * entry decision tree, *not* a linear funnel. Each card opens its own
 * short flow that can also be entered directly from elsewhere in the
 * app (keyboard shortcut, top-bar "+ NEW", empty-canvas hint).
 *
 * Architecture: this component is a thin renderer. It only emits a
 * `mode` selection back to the parent — every backend decision
 * (which recipes match each mode, whether there are existing teams,
 * what the "Add to team" picker should show) is owned by the
 * provider / runtime ops, not by this overlay.
 */

import type { Component } from "solid-js";
import { Show } from "solid-js";

export type CreateMode = "bot" | "team" | "addToTeam";

export interface ModeSelectOverlayProps {
  /**
   * Whether the colony has any teams already. The "Add to a Team"
   * card disables itself when false so the user doesn't enter a
   * picker that would be empty. Computed by the parent from the
   * existing dashboard state — not derived here.
   */
  hasExistingTeams: boolean;
  /** Fires when the user picks one of the three modes. */
  onSelectMode: (mode: CreateMode) => void;
  /** Fires when the user dismisses the hub. */
  onCancel: () => void;
}

interface ModeCardProps {
  glyph: string;
  title: string;
  description: string;
  disabled?: boolean;
  disabledReason?: string;
  onClick: () => void;
  recommended?: boolean;
}

const ModeCard: Component<ModeCardProps> = (props) => (
  <button
    type="button"
    class="colony-command-btn flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center transition"
    classList={{
      "is-disabled": props.disabled,
      "border-status-ok": props.recommended && !props.disabled,
    }}
    style={{ "min-height": "180px" }}
    disabled={props.disabled}
    title={props.disabled ? props.disabledReason : undefined}
    onClick={() => props.onClick()}
  >
    <div class="colony-text-2xl">{props.glyph}</div>
    <div class="colony-text-md font-bold text-text-primary">{props.title}</div>
    <div class="colony-text-2xs text-text-secondary">{props.description}</div>
    <Show when={props.disabled && props.disabledReason}>
      <div class="colony-text-3xs text-text-dim">{props.disabledReason}</div>
    </Show>
  </button>
);

export const ModeSelectOverlay: Component<ModeSelectOverlayProps> = (props) => {
  return (
    <div class="mx-auto max-w-3xl rounded border-2 border-bark bg-soil-mid p-6">
      <p class="colony-text-md font-bold text-text-primary">What would you like to make?</p>
      <p class="colony-text-xs mt-1 text-text-secondary">
        Pick one. You can change your mind or add more later.
      </p>

      <div class="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-3">
        <ModeCard
          glyph="🌱"
          title="Make a Bot"
          description="One agent that does one task."
          recommended
          onClick={() => props.onSelectMode("bot")}
        />
        <ModeCard
          glyph="👥"
          title="Make a Team"
          description="Multiple agents working together."
          onClick={() => props.onSelectMode("team")}
        />
        <ModeCard
          glyph="➕"
          title="Add to a Team"
          description="Pick a team you already built and add an agent."
          disabled={!props.hasExistingTeams}
          disabledReason="Make a team first."
          onClick={() => props.onSelectMode("addToTeam")}
        />
      </div>

      <div class="mt-6 flex justify-end">
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-4 py-2"
          onClick={() => props.onCancel()}
        >
          Cancel
        </button>
      </div>
    </div>
  );
};
