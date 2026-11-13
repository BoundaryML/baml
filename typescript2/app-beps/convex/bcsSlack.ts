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

/**
 * `text` cut to `max` characters, before escaping. Slack refuses a whole message when a section's
 * text passes 3,000 characters or a field's 2,000, so long notes and titles are shortened, with
 * room left for the words around them and for escaping, which can make text up to five times longer.
 */
function clip(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

function slackDate(ms: number): string {
  return `<!date^${Math.floor(ms / 1000)}^{date_short_pretty} at {time}|${new Date(ms).toISOString()}>`;
}

export interface Overview {
  number: number;
  title: string;
  status: string;
  authors: string[];
  shepherds?: string[];
  currentVersion: number;
  updatedAt: number;
  openComments?: number;
}

/** A comment as its Slack reply shows it. */
export interface SlackComment {
  id: string;
  threadId: string;
  body: string;
  isReply: boolean;
  page: string;
  pageTitle: string;
  quote?: string;
  threadAuthor: string;
  threadBody: string;
}

/** A link to a thread; the site finds its page and version, even after a page was removed. */
function commentUrl(number: number, comment: SlackComment): string {
  return `${SITE_URL}/bcs/${number}?comment=${comment.threadId}`;
}

/** A reply in the thread for a new comment or reply, quoting the block it is on. */
export function commentBlocks(proposal: Overview, who: string, comment: SlackComment): Block[] {
  const where = comment.page ? ` on *${escape(clip(comment.pageTitle, 100))}*` : "";
  const heading = comment.isReply
    ? `↪️ ${who} replied to *${escape(comment.threadAuthor)}*${where}`
    : `💬 ${who} commented${where}`;
  const quote = comment.isReply ? clip(comment.threadBody, 150) : comment.quote;
  const blocks: Block[] = [
    {
      type: "section",
      text: {
        type: "mrkdwn",
        text:
          heading +
          (quote ? `\n>_${escape(clip(quote, 200)).replace(/\n/g, " ")}_` : "") +
          `\n${escape(clip(comment.body, 1500))}`,
      },
    },
    {
      type: "actions",
      elements: [
        {
          type: "button",
          text: { type: "plain_text", text: comment.isReply ? "View thread" : "View & reply", emoji: true },
          url: commentUrl(proposal.number, comment),
        },
      ],
    },
  ];
  return blocks;
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
    { type: "section", text: { type: "mrkdwn", text: `*<${url}|${escape(clip(proposal.title, 300))}>*` } },
    {
      type: "section",
      fields: [
        { type: "mrkdwn", text: `*Status:* ${emoji} ${proposal.status}` },
        { type: "mrkdwn", text: `*Authors:* ${escape(clip(proposal.authors.join(", "), 300))}` },
        { type: "mrkdwn", text: `*Version:* v${proposal.currentVersion}` },
        ...(proposal.shepherds && proposal.shepherds.length > 0
          ? [{ type: "mrkdwn", text: `*Shepherds:* ${escape(clip(proposal.shepherds.join(", "), 300))}` }]
          : []),
        { type: "mrkdwn", text: `*Last updated:* ${slackDate(proposal.updatedAt)}` },
        ...(proposal.openComments !== undefined
          ? [{ type: "mrkdwn", text: `*Open comments:* ${proposal.openComments}` }]
          : []),
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
  change: {
    version?: number;
    note?: string;
    changed?: string[];
    status?: { from: string; to: string };
    shepherds?: string[];
  }
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
          // Which pages changed, when the proposal has more than its main page.
          (change.changed && (change.changed.length > 1 || change.changed[0] !== proposal.title)
            ? `\n_Changed: ${escape(clip(change.changed.join(", "), 300))}_`
            : "") +
          (change.note ? `\n>${escape(clip(change.note, 500)).replace(/\n/g, "\n>")}` : ""),
      },
    });
  }
  if (change.shepherds) {
    blocks.push({
      type: "section",
      text: {
        type: "mrkdwn",
        text:
          change.shepherds.length > 0
            ? `🧭 ${who} made *${escape(clip(change.shepherds.join(", "), 300))}* the shepherds of ${bcsId(proposal.number)}`
            : `🧭 ${who} removed the shepherds of ${bcsId(proposal.number)}`,
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
    const commentId =
      args.event.kind === "comment" || args.event.kind === "resolved" ? args.event.commentId : undefined;
    const proposal = await ctx.runQuery(internal.bcs.slackView, {
      docId: args.docId,
      version,
      githubId: args.githubId,
      commentId,
    });
    if (!proposal) return;
    const who = proposal.slackUserId ? `<@${proposal.slackUserId}>` : `*${escape(args.login)}*`;
    const created = args.event.kind === "created";
    const label = `${bcsId(proposal.number)}: ${clip(proposal.title, 300)}`;

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
      const kept = await ctx.runMutation(internal.bcs.storeSlackThread, {
        docId: args.docId,
        ts: posted.ts,
      });
      if (!kept) return;
      if (kept !== posted.ts) {
        // Another notification started the thread first: drop this message, bring that thread's
        // overview up to date (it may show the proposal as it was before this save), and reply there.
        await slack("chat.delete", { channel, ts: posted.ts });
        const latest = await ctx.runQuery(internal.bcs.slackView, { docId: args.docId });
        if (latest) {
          await slack("chat.update", {
            channel,
            ts: kept,
            blocks: overviewBlocks(latest, false),
            text: `${bcsId(latest.number)}: ${clip(latest.title, 300)}`,
          });
        }
      }
      thread = kept;
    }

    if (args.event.kind === "comment" && proposal.comment) {
      await slack("chat.postMessage", {
        channel,
        thread_ts: thread,
        blocks: commentBlocks(proposal, who, proposal.comment),
        text: `${proposal.comment.isReply ? "Reply" : "Comment"} on ${label}`,
        unfurl_links: false,
      });
      return;
    }
    if (args.event.kind === "resolved" && proposal.comment) {
      const verb = args.event.resolved ? "✅ resolved" : "↩️ reopened";
      await slack("chat.postMessage", {
        channel,
        thread_ts: thread,
        blocks: [
          {
            type: "section",
            text: {
              type: "mrkdwn",
              text: `${who} ${verb} <${commentUrl(proposal.number, proposal.comment)}|a comment> by *${escape(proposal.comment.threadAuthor)}*: _${escape(clip(proposal.comment.threadBody, 150)).replace(/\n/g, " ")}_`,
            },
          },
        ],
        text: `Comment ${args.event.resolved ? "resolved" : "reopened"} on ${label}`,
        unfurl_links: false,
      });
      return;
    }
    if (args.event.kind !== "saved") return;
    const blocks = savedBlocks(proposal, who, {
      version,
      note: proposal.note,
      changed: proposal.changed,
      status: args.event.status,
      shepherds: args.event.shepherds,
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
