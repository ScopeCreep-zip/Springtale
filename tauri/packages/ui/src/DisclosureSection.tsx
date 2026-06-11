/**
 * DisclosureSection — reusable progressive-disclosure block.
 *
 * Apple-print-dialog pattern: a `▸` chevron toggles a collapsed/
 * expanded state, revealing the children only when the user asks for
 * them. Used by `RecipeDeployPanel`, `ConnectorConfigPanel`, and
 * `RuleBuilderOverlay` to keep the same visual idiom across surfaces.
 *
 * Architecture note: the open/closed state is local (UI-only). The
 * fields revealed *inside* the children are still backend-supplied —
 * disclosure is just a render-time control, never a business-logic
 * decision.
 */

import type { Component, JSX } from "solid-js";
import { createSignal, Show } from "solid-js";

export interface DisclosureSectionProps {
  /** Short heading rendered next to the toggle chevron. */
  title: string;
  /** Optional sub-line rendered when collapsed (e.g. "4 optional fields"). */
  hint?: string;
  /** Default open state. Defaults to `false`. */
  defaultOpen?: boolean;
  /** Indent level — accumulates left padding when sections nest. */
  level?: number;
  /** Fires when the section toggles. Parents can use this to lazily
   *  fetch data they only want to load once the section opens. */
  onToggle?: (open: boolean) => void;
  children: JSX.Element;
}

export const DisclosureSection: Component<DisclosureSectionProps> = (props) => {
  const [open, setOpen] = createSignal(props.defaultOpen ?? false);
  const level = () => props.level ?? 0;
  const handleToggle = () => {
    const next = !open();
    setOpen(next);
    props.onToggle?.(next);
  };
  return (
    <div class="border-l-2 border-bark/40" style={{ "padding-left": `${level() * 12 + 8}px` }}>
      <button
        type="button"
        class="colony-text-2xs flex w-full items-center gap-2 py-2 text-left text-text-secondary hover:text-text-primary"
        onClick={handleToggle}
      >
        <span class="inline-block w-3">{open() ? "▼" : "▸"}</span>
        <span class="font-bold">{props.title}</span>
        <Show when={!open() && props.hint}>
          <span class="colony-text-3xs text-text-dim">{props.hint}</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="pb-2">{props.children}</div>
      </Show>
    </div>
  );
};
