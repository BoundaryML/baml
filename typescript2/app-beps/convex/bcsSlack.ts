// Slack messages for BCS proposals, like the BEP ones in slack.ts but in their own channel.
//
// Each proposal gets one thread in BCS_SLACK_CHANNEL: its first message is an overview (title,
// status, authors, version) that is edited as the proposal changes, and new versions and status
// changes are replies in the thread. Proposals are team only, so the channel must be private, with
// the bot behind SLACK_BOUNDARY_BOT_TOKEN invited. Nothing is posted until BCS_SLACK_CHANNEL is set.
import { v } from "convex/values";
import { internalAction } from "./_generated/server";
import { internal } from "./_generated/api";
import { slackEvent } from "./bcs";

/** Where proposal links point. */
const SITE_URL = "https://beps.boundaryml.com";

type Block = Record<string, unknown>;

interface Posted {
  ok: boolean;
  ts?: string;
  error?: string;
}

const STATUS_EMOJI: Record<string, string> = {
  draft: "📝",
  proposed: "📋",
  pending: "⏳",
  accepted: "✅",
  implemented: "🚀",
  rejected: "❌",
  superseded: "🔄",
};

function bcsId(number: number): string {
  return `BCS-${String(number).padStart(3, "0")}`;
}

function proposalUrl(number: number, version?: number): string {
  return `${SITE_URL}/bcs/${number}${version === undefined ? "" : `?v=${version}`}`;
}

/** Slack's mrkdwn treats &, < and > as markup. */
function escape(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function slackDate(ms: number): string {
  return `<!date^${Math.floor(ms / 1000)}^{date_short_pretty} at {time}|${new Date(ms).toISOString()}>`;
}

export interface Overview {
  number: number;
  title: string;
  status: string;
  authors: string[];
  currentVersion: number;
  updatedAt: number;
}

/** The thread's first message, edited in place whenever the proposal changes. */
export function overviewBlocks(proposal: Overview, isNew: boolean): Block[] {
  const url = proposalUrl(proposal.number);
  const emoji = STATUS_EMOJI[proposal.status] ?? "📄";
  return [
    {
      type: "header",
      text: {
        type: "plain_text",
        text: `${isNew ? "🆕 New BCS proposal: " : ""}${bcsId(proposal.number)}`,
        emoji: true,
      },
    },
    { type: "section", text: { type: "mrkdwn", text: `*<${url}|${escape(proposal.title)}>*` } },
    {
      type: "section",
      fields: [
        { type: "mrkdwn", text: `*Status:* ${emoji} ${proposal.status}` },
        { type: "mrkdwn", text: `*Authors:* ${escape(proposal.authors.join(", "))}` },
        { type: "mrkdwn", text: `*Version:* v${proposal.currentVersion}` },
        { type: "mrkdwn", text: `*Last updated:* ${slackDate(proposal.updatedAt)}` },
      ],
    },
    {
      type: "actions",
      elements: [
        {
          type: "button",
          text: { type: "plain_text", text: "Read the proposal", emoji: true },
          url,
          style: "primary",
        },
      ],
    },
  ];
}

/** A reply in the thread for a new version, a status change, or both from one save. */
export function savedBlocks(
  proposal: Overview,
  who: string,
  change: { version?: number; note?: string; status?: { from: string; to: string } }
): Block[] {
  const blocks: Block[] = [];
  if (change.version !== undefined) {
    const url = proposalUrl(proposal.number, change.version);
    blocks.push({
      type: "section",
      text: {
        type: "mrkdwn",
        text:
          `📝 ${who} saved <${url}|*v${change.version}*> of ${bcsId(proposal.number)}` +
          (change.note ? `\n>${escape(change.note)}` : ""),
      },
    });
  }
  if (change.status) {
    const emoji = STATUS_EMOJI[change.status.to] ?? "📄";
    blocks.push({
      type: "section",
      text: {
        type: "mrkdwn",
        text: `${emoji} ${who} moved ${bcsId(proposal.number)} from *${change.status.from}* to *${change.status.to}*`,
      },
    });
  }
  return blocks;
}

async function slack(method: string, body: Record<string, unknown>): Promise<Posted> {
  const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;
  if (!token) return { ok: false, error: "SLACK_BOUNDARY_BOT_TOKEN is not set" };
  try {
    const response = await fetch(`https://slack.com/api/${method}`, {
      method: "POST",
      headers: { "Content-Type": "application/json; charset=utf-8", Authorization: `Bearer ${token}` },
      body: JSON.stringify(body),
    });
    const result = (await response.json()) as Posted;
    if (!result.ok) console.error(`Slack ${method} failed for a BCS proposal:`, result.error);
    return result;
  } catch (error) {
    console.error(`Slack ${method} failed for a BCS proposal:`, error);
    return { ok: false, error: String(error) };
  }
}

/** Posts or updates a proposal's thread after it was created or saved. */
export const notify = internalAction({
  args: {
    docId: v.id("bcsDocs"),
    event: slackEvent,
    login: v.string(),
    githubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const channel = process.env.BCS_SLACK_CHANNEL?.trim();
    if (!channel) return;
    const version = args.event.kind === "saved" ? args.event.version : undefined;
    const proposal = await ctx.runQuery(internal.bcs.slackView, {
      docId: args.docId,
      version,
      githubId: args.githubId,
    });
    if (!proposal) return;
    const who = proposal.slackUserId ? `<@${proposal.slackUserId}>` : `*${escape(args.login)}*`;
    const created = args.event.kind === "created";
    const label = `${bcsId(proposal.number)}: ${proposal.title}`;

    let thread = proposal.slackThreadTs;
    if (thread) {
      await slack("chat.update", {
        channel,
        ts: thread,
        blocks: overviewBlocks(proposal, false),
        text: label,
      });
    } else {
      // The first message of the thread: on creation, or on the first save after the channel was set.
      const posted = await slack("chat.postMessage", {
        channel,
        blocks: overviewBlocks(proposal, created),
        text: created ? `New BCS proposal: ${label}` : label,
        unfurl_links: false,
      });
      if (!posted.ok || !posted.ts) return;
      thread = posted.ts;
      await ctx.runMutation(internal.bcs.storeSlackThread, { docId: args.docId, ts: thread });
    }

    if (args.event.kind !== "saved") return;
    const blocks = savedBlocks(proposal, who, {
      version,
      note: proposal.note,
      status: args.event.status,
    });
    if (blocks.length === 0) return;
    await slack("chat.postMessage", {
      channel,
      thread_ts: thread,
      blocks,
      text: `${label} changed`,
      unfurl_links: false,
    });
  },
});
