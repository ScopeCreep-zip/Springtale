import type { CooperationEventEnvelope, UtteranceDefs } from "../../dashboard/types";
import { get } from "./client";
import { type SseState, subscribeSse } from "./sse";

/** Plan §1.15 G — the utterance def table (`GET /cooperation/utterances`). */
export async function getUtteranceDefs(): Promise<UtteranceDefs> {
  return get<UtteranceDefs>("/cooperation/utterances");
}

/**
 * Phase H — cooperation lifecycle events: the `cooperation` frames of the
 * multiplexed `GET /stream` (ticket-authenticated; see `sse.ts`). The web
 * dashboard uses this; desktop uses the Tauri `subscribe_cooperation`
 * Channel<CooperationEventEnvelope> via `apps/desktop/src/provider.ts`
 * instead (per E10 — Channel<T> beats SSE on Tauri for high-rate streams).
 */
export function subscribeToCooperationEvents(
  baseUrl: string,
  onEvent: (envelope: CooperationEventEnvelope) => void,
  onState?: (s: SseState) => void,
): () => void {
  return subscribeSse(
    "/stream",
    (name, data) => {
      if (name === "cooperation") onEvent(data as CooperationEventEnvelope);
    },
    onState,
    baseUrl,
  );
}
