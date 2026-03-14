"use node";

import { v } from "convex/values";
import { internalAction } from "./_generated/server";
import { internal } from "./_generated/api";

const SLACK_CHANNEL = "#beps";

interface SlackBlock {
  type: string;
  text?: {
    type: string;
    text: string;
    emoji?: boolean;
  };
  fields?: Array<{
    type: string;
    text: string;
  }>;
  elements?: Array<{
    type: string;
    text: string;
    url?: string;
  }>;
}

interface SlackPostResponse {
  ok: boolean;
  ts?: string;
  error?: string;
}

interface SlackUserLookupResponse {
  ok: boolean;
  user?: {
    id: string;
    name: string;
    real_name?: string;
  };
  error?: string;
}

async function postToSlack(
  blocks: SlackBlock[],
  text: string,
  threadTs?: string
): Promise<SlackPostResponse> {
  const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;

  if (!token) {
    console.warn("SLACK_BOUNDARY_BOT_TOKEN not configured - skipping Slack notification");
    return { ok: false, error: "SLACK_BOUNDARY_BOT_TOKEN not configured" };
  }

  const payload: Record<string, unknown> = {
    channel: SLACK_CHANNEL,
    blocks,
    text,
  };

  if (threadTs) {
    payload.thread_ts = threadTs;
  }

  try {
    const response = await fetch("https://slack.com/api/chat.postMessage", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify(payload),
    });

    const result = (await response.json()) as SlackPostResponse;

    if (!result.ok) {
      console.error("Slack API error:", result.error);
    }

    return result;
  } catch (error) {
    console.error("Failed to post to Slack:", error);
    return { ok: false, error: String(error) };
  }
}

function getBepUrl(bepNumber: number): string {
  const baseUrl = process.env.NEXT_PUBLIC_CONVEX_URL
    ? process.env.NEXT_PUBLIC_CONVEX_URL.replace(".convex.cloud", ".vercel.app")
    : "https://beps.boundaryml.com";
  return `${baseUrl}/beps/${bepNumber}`;
}

function getStatusEmoji(status: string): string {
  switch (status) {
    case "draft":
      return "📝";
    case "proposed":
      return "📋";
    case "accepted":
      return "✅";
    case "implemented":
      return "🚀";
    case "rejected":
      return "❌";
    case "superseded":
      return "🔄";
    default:
      return "📄";
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// INTERNAL ACTIONS: Slack notifications
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Notify Slack when a new BEP is created.
 * Creates a new thread in #beps and stores the thread_ts.
 * If shepherds have linked Slack accounts, mentions them directly.
 */
export const notifyBepCreated = internalAction({
  args: {
    bepId: v.id("beps"),
  },
  handler: async (ctx, args) => {
    const bep = await ctx.runQuery(internal.beps.getById, { id: args.bepId });
    if (!bep) {
      console.error("BEP not found for Slack notification:", args.bepId);
      return;
    }

    const shepherds: string[] = [];
    for (const shepherdId of bep.shepherds) {
      const user = await ctx.runQuery(internal.users.getById, { id: shepherdId });
      if (user) {
        shepherds.push(formatAuthorForSlack(user.name, user.slackUserId));
      }
    }

    const bepUrl = getBepUrl(bep.number);
    const statusEmoji = getStatusEmoji(bep.status);

    const blocks: SlackBlock[] = [
      {
        type: "header",
        text: {
          type: "plain_text",
          text: `🆕 New BEP Created: BEP-${bep.number}`,
          emoji: true,
        },
      },
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `*<${bepUrl}|${bep.title}>*`,
        },
      },
      {
        type: "section",
        fields: [
          {
            type: "mrkdwn",
            text: `*Status:* ${statusEmoji} ${bep.status}`,
          },
          {
            type: "mrkdwn",
            text: `*Shepherds:* ${shepherds.length > 0 ? shepherds.join(", ") : "None assigned"}`,
          },
        ],
      },
    ];

    if (bep.content) {
      const preview = bep.content.substring(0, 300) + (bep.content.length > 300 ? "..." : "");
      blocks.push({
        type: "section",
        text: {
          type: "mrkdwn",
          text: `_${preview}_`,
        },
      });
    }

    blocks.push({
      type: "section",
      text: {
        type: "mrkdwn",
        text: `<${bepUrl}|View BEP-${bep.number} →>`,
      },
    });

    const result = await postToSlack(
      blocks,
      `New BEP Created: BEP-${bep.number} - ${bep.title}`
    );

    if (result.ok && result.ts) {
      await ctx.runMutation(internal.beps.storeSlackThreadTs, {
        bepId: args.bepId,
        slackThreadTs: result.ts,
      });
    }
  },
});

/**
 * Notify Slack when a BEP is updated with a new version.
 * Replies to the existing thread if one exists.
 * If the editor has a linked Slack account, mentions them directly.
 */
export const notifyBepVersionCreated = internalAction({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
  },
  handler: async (ctx, args) => {
    const bep = await ctx.runQuery(internal.beps.getById, { id: args.bepId });
    if (!bep) {
      console.error("BEP not found for Slack notification:", args.bepId);
      return;
    }

    const version = await ctx.runQuery(internal.beps.getVersionById, {
      id: args.versionId,
    });
    if (!version) {
      console.error("Version not found for Slack notification:", args.versionId);
      return;
    }

    const bepUrl = getBepUrl(bep.number);
    const editorDisplay = formatAuthorForSlack(version.editedByName, version.editedBySlackUserId);

    const blocks: SlackBlock[] = [
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `📝 *Version ${version.version}* published for <${bepUrl}|BEP-${bep.number}>`,
        },
      },
      {
        type: "section",
        fields: [
          {
            type: "mrkdwn",
            text: `*Edited by:* ${editorDisplay}`,
          },
          {
            type: "mrkdwn",
            text: `*Note:* ${version.editNote || "No description"}`,
          },
        ],
      },
    ];

    const result = await postToSlack(
      blocks,
      `BEP-${bep.number} updated to version ${version.version}`,
      bep.slackThreadTs
    );

    if (!bep.slackThreadTs && result.ok && result.ts) {
      await ctx.runMutation(internal.beps.storeSlackThreadTs, {
        bepId: args.bepId,
        slackThreadTs: result.ts,
      });
    }
  },
});

/**
 * Format author name for Slack - mentions their Slack account if linked.
 */
function formatAuthorForSlack(authorName: string, slackUserId?: string): string {
  if (slackUserId) {
    return `<@${slackUserId}>`;
  }
  return `*${authorName}*`;
}

/**
 * Notify Slack when a comment is added to a BEP.
 * Replies to the existing thread if one exists.
 * If the author has a linked Slack account, mentions them directly.
 */
export const notifyCommentAdded = internalAction({
  args: {
    bepId: v.id("beps"),
    commentId: v.id("comments"),
  },
  handler: async (ctx, args) => {
    const bep = await ctx.runQuery(internal.beps.getById, { id: args.bepId });
    if (!bep) {
      console.error("BEP not found for Slack notification:", args.bepId);
      return;
    }

    const comment = await ctx.runQuery(internal.comments.getById, {
      id: args.commentId,
    });
    if (!comment) {
      console.error("Comment not found for Slack notification:", args.commentId);
      return;
    }

    const bepUrl = getBepUrl(bep.number);
    const authorDisplay = formatAuthorForSlack(comment.authorName, comment.authorSlackUserId);

    const typeEmoji = {
      discussion: "💬",
      concern: "⚠️",
      question: "❓",
    }[comment.type] || "💬";

    const blocks: SlackBlock[] = [
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `${typeEmoji} ${authorDisplay} ${comment.parentId ? "replied to a comment" : `added a ${comment.type}`} on <${bepUrl}|BEP-${bep.number}>`,
        },
      },
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `> ${comment.content.substring(0, 500)}${comment.content.length > 500 ? "..." : ""}`,
        },
      },
    ];

    const result = await postToSlack(
      blocks,
      `${comment.authorName} commented on BEP-${bep.number}`,
      bep.slackThreadTs
    );

    if (!bep.slackThreadTs && result.ok && result.ts) {
      await ctx.runMutation(internal.beps.storeSlackThreadTs, {
        bepId: args.bepId,
        slackThreadTs: result.ts,
      });
    }
  },
});

/**
 * Notify Slack when a BEP's status changes.
 * Replies to the existing thread if one exists.
 */
export const notifyStatusChanged = internalAction({
  args: {
    bepId: v.id("beps"),
    newStatus: v.string(),
  },
  handler: async (ctx, args) => {
    const bep = await ctx.runQuery(internal.beps.getById, { id: args.bepId });
    if (!bep) {
      console.error("BEP not found for Slack notification:", args.bepId);
      return;
    }

    const bepUrl = getBepUrl(bep.number);
    const statusEmoji = getStatusEmoji(args.newStatus);

    const blocks: SlackBlock[] = [
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `${statusEmoji} <${bepUrl}|BEP-${bep.number}> status changed to *${args.newStatus}*`,
        },
      },
    ];

    const result = await postToSlack(
      blocks,
      `BEP-${bep.number} status changed to ${args.newStatus}`,
      bep.slackThreadTs
    );

    if (!bep.slackThreadTs && result.ok && result.ts) {
      await ctx.runMutation(internal.beps.storeSlackThreadTs, {
        bepId: args.bepId,
        slackThreadTs: result.ts,
      });
    }
  },
});

/**
 * Look up a Slack user by email and link them to a BEPS user.
 * This is called when a special account (BoundaryML team member) logs in.
 */
export const lookupAndLinkSlackUser = internalAction({
  args: {
    userId: v.id("users"),
    email: v.string(),
  },
  handler: async (ctx, args) => {
    const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;

    if (!token) {
      console.warn("SLACK_BOUNDARY_BOT_TOKEN not configured - skipping Slack user lookup");
      return;
    }

    try {
      const response = await fetch(
        `https://slack.com/api/users.lookupByEmail?email=${encodeURIComponent(args.email)}`,
        {
          method: "GET",
          headers: {
            Authorization: `Bearer ${token}`,
          },
        }
      );

      const result = (await response.json()) as SlackUserLookupResponse;

      if (result.ok && result.user) {
        console.log(`Found Slack user ${result.user.id} (${result.user.real_name || result.user.name}) for email ${args.email}`);
        await ctx.runMutation(internal.users.linkSlackUserId, {
          userId: args.userId,
          slackUserId: result.user.id,
        });
      } else {
        console.warn(`Could not find Slack user for email ${args.email}: ${result.error}`);
      }
    } catch (error) {
      console.error("Failed to lookup Slack user:", error);
    }
  },
});
