import type { IssueEvent } from "./db";
import { slackChannelUrl } from "./slack";

export function validProposalId(id: string): boolean {
  return /^[a-zA-Z0-9_-]{1,80}$/.test(id);
}

export function proposalFromEvents(events: IssueEvent[]) {
  const created = events.find(e => e.kind === "babysit_proposed");
  if (!created || typeof created.payload.proposed_fix !== "string") return null;
  const posted = events.find(e => e.kind === "babysit_proposal_posted");
  const base = slackChannelUrl();
  const channel = posted?.payload.channel;
  const ts = posted?.payload.message_ts;
  const slackUrl = base && typeof channel === "string" && /^[CG][A-Z0-9]+$/.test(channel)
    && typeof ts === "string" && /^[0-9]+\.[0-9]+$/.test(ts)
    ? `${new URL(base).origin}/archives/${channel}/p${ts.replace(".", "")}` : base;
  return {
    fix: created.payload.proposed_fix,
    head: typeof created.payload.head === "string" ? created.payload.head : null,
    slackUrl,
    started: events.some(e => ["babysit_approved", "babysit_fix_started"].includes(e.kind)),
    pushing: events.some(e => e.kind === "babysit_pushing"),
  };
}
