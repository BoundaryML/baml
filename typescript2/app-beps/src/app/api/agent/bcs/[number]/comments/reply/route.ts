// A reply to a comment thread, from an agent or `bep bcs reply`, as BEP agents reply on a BEP:
// signed by the member whose token sent it, and posted to the proposal's Slack thread like any
// reply. A reply to a reply joins the same thread; a reply to a resolved thread reopens it. An
// agent writing for the member names itself (`agent`: claude, codex, ...), and the site and Slack
// show the reply as the member's agent's.
import { NextRequest } from "next/server";
import { api } from "../../../../../../../../convex/_generated/api";
import { bcsCall } from "@/lib/bcs-convex";
import { authenticate, json, refused } from "../../../shared";
import { KNOWN_AGENTS, KNOWN_AGENT_IDS, normalizeAgent } from "../../../../../../../../convex/bcsAgents";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = { params: Promise<{ number: string }> };

/** Saves `{ commentId, body, agent? }` (`content` for `body`, as the BEP route names it) as a reply. */
export async function POST(request: NextRequest, { params }: Params) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const number = Number((await params).number);
  const sent = await request.json().catch(() => null);
  if (!Number.isInteger(number) || !sent) return json({ error: "Send a JSON body." }, 400);
  const body = typeof sent.body === "string" ? sent.body : sent.content;
  if (typeof sent.commentId !== "string" || typeof body !== "string") {
    return json({ error: "Send { commentId, body }." }, 400);
  }
  if (sent.agent !== undefined && sent.agent !== null && typeof sent.agent !== "string") {
    return json({ error: "agent must name the agent, such as claude." }, 400);
  }
  const replied = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.reply, {
      secret,
      number,
      commentId: sent.commentId,
      body,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      agent: sent.agent ?? undefined,
    })
  );
  if (!replied.ok) return refused(replied.failure);
  return json({
    commentId: replied.value,
    author: viewer.login,
    ...agentAnswer(sent.agent),
    url: `${request.nextUrl.origin}/bcs/${number}?comment=${replied.value}`,
  });
}

/**
 * The agent a reply was stored as, and for a name that is not a known agent's, a note the CLI shows:
 * an agent that misnamed itself can use a known name next time.
 */
function agentAnswer(given: unknown) {
  if (typeof given !== "string" || !given.trim()) return {};
  const agent = normalizeAgent(given);
  if (KNOWN_AGENTS[agent]) return { agent };
  return {
    agent,
    note: `"${agent}" is not a known agent, so it shows with a generic icon. Known agents: ${KNOWN_AGENT_IDS}.`,
  };
}
