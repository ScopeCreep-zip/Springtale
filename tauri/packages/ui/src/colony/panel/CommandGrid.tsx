import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
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
 * W6 Nintendo 3-action grid for the frontend-owned command contexts.
 * Leads with the (≤3) `primary` commands; the rest are revealed by a MORE
 * toggle. When a context has no primaries flagged (or ≤3 real commands
 * total), every command shows and the toggle is hidden — so this is a safe
 * superset of the old "show everything" behaviour.
 */
export const CommandGridStatic: Component<{
  commands: (ColonyCommand | null)[] | undefined;
  onCommand: (action: string) => void;
}> = (props) => {
  const [expanded, setExpanded] = createSignal(false);

  const real = () => (props.commands ?? []).filter((c): c is ColonyCommand => c !== null);
  const primaries = () => real().filter((c) => c.primary);
  // No primaries flagged → treat all as visible (legacy contexts).
  const hasDrawer = () => primaries().length > 0 && real().length > primaries().length;
  const visible = () => {
    if (!hasDrawer() || expanded()) return real();
    return primaries();
  };

  const button = (cmd: ColonyCommand) => (
    <button type="button" class="colony-command-btn" onClick={() => props.onCommand(cmd.action)}>
      <span class="colony-text-icon">{cmd.icon}</span>
      {cmd.label}
      <span class="colony-text-3xs bg-soil-deep px-0.5 text-text-dim">{cmd.key}</span>
    </button>
  );

  return (
    <div class="grid h-[calc(100%-16px)] grid-cols-3 gap-0.5">
      <For each={visible()}>{(cmd) => button(cmd)}</For>
      <Show when={hasDrawer()}>
        <button
          type="button"
          class="colony-command-btn"
          onClick={() => setExpanded((v) => !v)}
          title={expanded() ? "Show fewer actions" : "Show more actions"}
        >
          <span class="colony-text-icon">{expanded() ? "<" : "…"}</span>
          {expanded() ? "LESS" : "MORE"}
        </button>
      </Show>
    </div>
  );
};

// ── List Views ───────────────────────────────────────────
