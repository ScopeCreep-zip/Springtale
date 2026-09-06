import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { CommandDecl } from "../../dashboard/types";
import type { ColonyCommand, ColonySelection } from "../types";
import { COMMANDS } from "../types";

export const CommandGrid: Component<{
  selection: ColonySelection;
  onCommand: (label: string) => void;
  formationCommands?: CommandDecl[];
}> = (props) => {
  // F1: formation context renders ONLY backend-supplied commands (B11);
  // there is no static fallback. Other selection contexts use the
  // hardcoded `COMMANDS` table for now (those grids are still
  // frontend-owned per colony-canvas.md §3 — only formation commands
  // need backend status-awareness).
  return (
    <Show
      when={props.selection.type !== "formation"}
      fallback={
        <div class="grid h-[calc(100%-16px)] grid-cols-3 gap-0.5">
          <For each={props.formationCommands ?? []}>
            {(cmd) => (
              <button
                type="button"
                class="colony-command-btn"
                classList={{ "is-disabled": !cmd.enabled }}
                disabled={!cmd.enabled}
                title={cmd.disabled_reason ?? cmd.label}
                onClick={() => props.onCommand(cmd.id)}
              >
                <span class="colony-text-icon">{cmd.icon}</span>
                {cmd.label}
                <span class="colony-text-3xs bg-soil-deep px-0.5 text-text-dim">{cmd.hotkey}</span>
              </button>
            )}
          </For>
        </div>
      }
    >
      <CommandGridStatic
        commands={COMMANDS[props.selection.type ?? "none"]}
        onCommand={props.onCommand}
      />
    </Show>
  );
};

/**
 * The static 3×3 command card for the frontend-owned contexts.
 *
 * Plan 3.3: every slot is fixed, the whole card is always visible (no MORE /
 * LESS paging), and a slot with no verb renders as an empty cell rather than
 * an empty button — so the verbs that do exist never move between contexts.
 */
export const CommandGridStatic: Component<{
  commands: (ColonyCommand | null)[] | undefined;
  onCommand: (action: string) => void;
}> = (props) => {
  return (
    <div class="grid h-[calc(100%-16px)] grid-cols-3 gap-0.5">
      <For each={props.commands ?? []}>
        {(slot) => (
          <Show when={slot} fallback={<div aria-hidden="true" />}>
            {(cmd) => (
              <button
                type="button"
                class="colony-command-btn"
                onClick={() => props.onCommand(cmd().action)}
              >
                <span class="colony-text-icon">{cmd().icon}</span>
                {cmd().label}
                <span class="colony-text-3xs bg-soil-deep px-0.5 text-text-dim">{cmd().key}</span>
              </button>
            )}
          </Show>
        )}
      </For>
    </div>
  );
};

// ── List Views ───────────────────────────────────────────
