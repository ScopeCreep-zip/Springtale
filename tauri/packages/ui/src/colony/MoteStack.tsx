/**
 * MoteStack — the utterance renderer (ALIGNMENT-PLAN 1.15 / 3.4).
 *
 * Every mark is text from the shipped "Springtale Symbols" font subset,
 * every severity an ISO 3864 shape (triangle warning, circle prohibition,
 * square information; colour fixed to shape), every slot an `aria-label`
 * from the locale dictionary (WCAG 1.4.1: colour is never the only cue).
 * Motes expire on the colony tick clock (`seq + ttl_ticks > now`), never on
 * a wall-clock timer. Limits from RimWorld Bubbles (three per pawn, scale
 * cap 1.25, hidden past the zoom-out altitude); frame timing from Stardew
 * (250 ms per hold frame, 3 x 20 ms pop). Directional glyphs mirror under RTL.
 */

import { type Component, createMemo, For, Show } from "solid-js";
import { agentMatches } from "../dashboard/activity";
import type { Utterance } from "../dashboard/types";
import { useI18n } from "../i18n/context";
import type { Locale } from "../i18n/types";
import type { ColonyAgent } from "./types";
import { ROLE_GLYPHS } from "./types";

/** One colony tick: `CadenceBus::new(Duration::from_secs(1), …)` in lifecycle.rs. */
export const TICK_MS = 1000;

/** RimWorld Bubbles `PawnMax`. */
const MAX_PER_AGENT = 3;
/** RimWorld Bubbles `ScaleMax`. */
const SCALE_MAX = 1.25;
/** RimWorld Bubbles `AltitudeMax`, as the view scale below which motes hide. */
const ALTITUDE_MIN_SCALE = 0.4;

export interface MoteStackProps {
  agent: ColonyAgent;
  utterances: Utterance[];
  /** The colony tick clock (`colonyNow`), not wall-clock. */
  now: number;
  agentToConnector: Record<string, string>;
  framesFor: (u: Utterance, locale: Locale) => string[];
  roleOf: (agentId: string) => string | undefined;
  viewScale: number;
}

export const MoteStack: Component<MoteStackProps> = (p) => {
  const { t, locale, dir } = useI18n();
  // Newest three, unexpired on the colony timeline.
  const live = createMemo(() =>
    p.utterances
      .filter((u) => agentMatches(u, p.agent, p.agentToConnector) && u.seq + u.ttl_ticks > p.now)
      .sort((a, b) => b.seq - a.seq)
      .slice(0, MAX_PER_AGENT),
  );
  const scale = () => Math.min(p.viewScale, SCALE_MAX);
  return (
    <Show when={p.viewScale >= ALTITUDE_MIN_SCALE && live().length > 0}>
      <div class="mote-stack" style={{ transform: `translateX(-50%) scale(${scale()})` }}>
        <For each={live()}>
          {(u) => {
            const frames = () => {
              const f = p.framesFor(u, locale());
              // The Sims: the icon of the thing you're thinking of.
              if (u.utterance.utter === "yield" && f[0] === "role") {
                const role = p.roleOf(u.utterance.beneficiary);
                return [role ? (ROLE_GLYPHS[role] ?? "·") : "·"];
              }
              return f;
            };
            const mirrored = () => u.mirror_rtl && dir() === "rtl";
            return (
              <div
                class="mote-slot"
                role="img"
                aria-label={t(u.label_key)}
                style={{ "--colony-mote-ttl": `${u.ttl_ticks * TICK_MS}ms` }}
              >
                <div
                  class="mote-shape"
                  data-shape={u.shape}
                  data-carrier={u.carrier}
                  data-tone={u.tone}
                >
                  <div class="mote-frames" data-n={frames().length}>
                    <For each={frames()}>
                      {(g, i) => (
                        <span
                          class="mote-glyph"
                          classList={{ "is-mirrored": mirrored() }}
                          style={{ "--colony-mote-i": i() }}
                          aria-hidden="true"
                        >
                          {g}
                        </span>
                      )}
                    </For>
                  </div>
                </div>
              </div>
            );
          }}
        </For>
      </div>
    </Show>
  );
};
