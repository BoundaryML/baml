// Fire-and-forget run logging into the Convex "experiments" project.
// Set CONVEX_URL to disable/point elsewhere; failures never break a turn.

import { ConvexHttpClient } from "convex/browser";
import { anyApi } from "convex/server";

const url = process.env.CONVEX_URL;
const client = url ? new ConvexHttpClient(url) : null;

export interface TurnLog {
  source: "arena" | "compare";
  suite?: string;
  caseId?: string;
  agent: string;
  utterance: string;
  toolCalls: { name: string; args: any; tool: string; data: any; say: string }[];
  finalText: string;
  ms: number | null;
  pass?: boolean;
  detail?: string;
}

export function logTurn(turn: TurnLog): void {
  if (!client) return;
  client
    .mutation(anyApi.turns.log, {
      ...turn,
      realtimeModel: process.env.REALTIME_MODEL ?? "gpt-realtime-2.1",
      thinkerModel: process.env.THINKER_MODEL,
    })
    .catch((err) => console.error("[convex] log failed:", String(err).slice(0, 200)));
}
