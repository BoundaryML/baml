"use node";

import { v } from "convex/values";
import { internalAction } from "./_generated/server";
import { internal } from "./_generated/api";

const SLACK_CHANNEL = "#beps";

interface SlackTextObject {
  type: "plain_text" | "mrkdwn";
  text: string;
  emoji?: boolean;
}

interface SlackButtonElement {
  type: "button";
  text: SlackTextObject;
  url?: string;
  action_id?: string;
  value?: string;
  style?: "primary" | "danger";
}

interface SlackContextElement {
  type: "plain_text" | "mrkdwn" | "image";
  text?: string;
  image_url?: string;
  alt_text?: string;
}

interface SlackBlock {
  type: "section" | "header" | "divider" | "context" | "actions";
  text?: SlackTextObject;
  fields?: SlackTextObject[];
  elements?: (SlackButtonElement | SlackContextElement)[];
  accessory?: SlackButtonElement;
  block_id?: string;
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

async function updateSlackMessage(
  ts: string,
  blocks: SlackBlock[],
  text: string
): Promise<SlackPostResponse> {
  const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;

  if (!token) {
    console.warn("SLACK_BOUNDARY_BOT_TOKEN not configured - skipping Slack update");
    return { ok: false, error: "SLACK_BOUNDARY_BOT_TOKEN not configured" };
  }

  const payload = {
    channel: SLACK_CHANNEL,
    ts,
    blocks,
    text,
  };

  try {
    const response = await fetch("https://slack.com/api/chat.update", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify(payload),
    });

    const result = (await response.json()) as SlackPostResponse;

    if (!result.ok) {
      console.error("Slack API update error:", result.error);
    }

    return result;
  } catch (error) {
    console.error("Failed to update Slack message:", error);
    return { ok: false, error: String(error) };
  }
}

function getBepUrl(bepNumber: number): string {
  const baseUrl = process.env.NEXT_PUBLIC_CONVEX_URL
    ? process.env.NEXT_PUBLIC_CONVEX_URL.replace(".convex.cloud", ".vercel.app")
    : "https://beps.boundaryml.com";
  return `${baseUrl}/beps/${bepNumber}`;
}

function getCommentPermalink(bepNumber: number, commentId: string): string {
  return `${getBepUrl(bepNumber)}?focusComment=${commentId}`;
}

function getStatusEmoji(status: string): string {
  switch (status) {
    case "draft":
      return "📝";
    case "proposed":
      return "📋";
    case "pending":
      return "⏳";
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
 * Build the overview blocks for a BEP thread.
 * This is used both for creating new threads and updating existing ones.
 */
interface BepOverviewData {
  number: number;
  title: string;
  status: string;
  updatedAt: number;
  author: { name: string; slackUserId?: string };
  shepherds: Array<{ name: string; slackUserId?: string }>;
  commenters: Array<{ name: string; slackUserId?: string }>;
  lastComment: { authorName: string; authorSlackUserId?: string; createdAt: number } | null;
  commentCount: number;
}

function buildOverviewBlocks(overview: BepOverviewData, isNew: boolean): SlackBlock[] {
  const bepUrl = getBepUrl(overview.number);
  const statusEmoji = getStatusEmoji(overview.status);

  const authorDisplay = formatAuthorForSlack(overview.author.name, overview.author.slackUserId);
  const shepherdDisplay = overview.shepherds.length > 0
    ? overview.shepherds.map(s => formatAuthorForSlack(s.name, s.slackUserId)).join(", ")
    : "None assigned";

  const headerText = isNew
    ? `🆕 New BEP Created: BEP-${overview.number}`
    : `📋 BEP-${overview.number}`;

  const blocks: SlackBlock[] = [
    {
      type: "header",
      text: {
        type: "plain_text",
        text: headerText,
        emoji: true,
      },
    },
    {
      type: "section",
      text: {
        type: "mrkdwn",
        text: `*<${bepUrl}|${overview.title}>*`,
      },
    },
    {
      type: "section",
      fields: [
        {
          type: "mrkdwn",
          text: `*Status:* ${statusEmoji} ${overview.status}`,
        },
        {
          type: "mrkdwn",
          text: `*Author:* ${authorDisplay}`,
        },
        {
          type: "mrkdwn",
          text: `*Shepherds:* ${shepherdDisplay}`,
        },
        {
          type: "mrkdwn",
          text: `*Last Updated:* <!date^${Math.floor(overview.updatedAt / 1000)}^{date_short_pretty} at {time}|${new Date(overview.updatedAt).toISOString()}>`,
        },
      ],
    },
  ];

  // Add comment info if there are any comments
  if (overview.commentCount > 0 && overview.lastComment) {
    const commenterDisplay = overview.commenters.length > 0
      ? overview.commenters.slice(0, 5).map(c => formatAuthorForSlack(c.name, c.slackUserId)).join(", ")
        + (overview.commenters.length > 5 ? ` +${overview.commenters.length - 5} more` : "")
      : "None";

    const lastCommenterDisplay = formatAuthorForSlack(
      overview.lastComment.authorName,
      overview.lastComment.authorSlackUserId
    );

    blocks.push({
      type: "section",
      fields: [
        {
          type: "mrkdwn",
          text: `*Commented by:* ${commenterDisplay}`,
        },
        {
          type: "mrkdwn",
          text: `*Last Comment:* ${lastCommenterDisplay} on <!date^${Math.floor(overview.lastComment.createdAt / 1000)}^{date_short_pretty} at {time}|${new Date(overview.lastComment.createdAt).toISOString()}>`,
        },
      ],
    });
  }

  blocks.push({
    type: "context",
    elements: [
      {
        type: "mrkdwn",
        text: `${overview.commentCount} comment${overview.commentCount !== 1 ? "s" : ""} | <${bepUrl}|View BEP-${overview.number} →>`,
      },
    ],
  });

  return blocks;
}

/**
 * Notify Slack when a new BEP is created.
 * Creates a new thread in #beps with the overview and stores the thread_ts.
 */
export const notifyBepCreated = internalAction({
  args: {
    bepId: v.id("beps"),
  },
  handler: async (ctx, args) => {
    const overview = await ctx.runQuery(internal.beps.getBepOverview, { bepId: args.bepId });
    if (!overview) {
      console.error("BEP not found for Slack notification:", args.bepId);
      return;
    }

    const blocks = buildOverviewBlocks(overview, true);

    const result = await postToSlack(
      blocks,
      `New BEP Created: BEP-${overview.number} - ${overview.title}`
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
 * Update the Slack thread overview message for a BEP.
 * This is called when the BEP is updated, commented on, or status changes.
 */
export const updateBepThreadOverview = internalAction({
  args: {
    bepId: v.id("beps"),
  },
  handler: async (ctx, args) => {
    const overview = await ctx.runQuery(internal.beps.getBepOverview, { bepId: args.bepId });
    if (!overview) {
      console.error("BEP not found for Slack thread update:", args.bepId);
      return;
    }

    if (!overview.slackThreadTs) {
      console.log("No Slack thread for BEP, creating one:", args.bepId);
      const blocks = buildOverviewBlocks(overview, false);
      const result = await postToSlack(
        blocks,
        `BEP-${overview.number} - ${overview.title}`
      );
      if (result.ok && result.ts) {
        await ctx.runMutation(internal.beps.storeSlackThreadTs, {
          bepId: args.bepId,
          slackThreadTs: result.ts,
        });
      }
      return;
    }

    const blocks = buildOverviewBlocks(overview, false);

    await updateSlackMessage(
      overview.slackThreadTs,
      blocks,
      `BEP-${overview.number} - ${overview.title}`
    );
  },
});

/**
 * Notify Slack when a BEP is updated with a new version.
 * Replies to the existing thread if one exists.
 * If the editor has a linked Slack account, mentions them directly.
 * Also updates the thread overview message.
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

    // Update the thread overview with new "last updated" info
    await ctx.runAction(internal.slack.updateBepThreadOverview, { bepId: args.bepId });
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
 * Get a human-readable label for the comment type.
 */
function getCommentTypeLabel(type: string): { emoji: string; label: string; color: string } {
  switch (type) {
    case "concern":
      return { emoji: "⚠️", label: "Concern", color: "#e74c3c" };
    case "question":
      return { emoji: "❓", label: "Question", color: "#3498db" };
    case "discussion":
    default:
      return { emoji: "💬", label: "Discussion", color: "#2ecc71" };
  }
}

/**
 * Notify Slack when a comment is added to a BEP.
 * Replies to the existing thread if one exists.
 * If the author has a linked Slack account, mentions them directly.
 * 
 * Improved formatting with:
 * - Type badge and context info
 * - Clean quote formatting
 * - Permalink to comment
 * - Action buttons
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
    const commentPermalink = getCommentPermalink(bep.number, args.commentId);
    const authorDisplay = formatAuthorForSlack(comment.authorName, comment.authorSlackUserId);
    const typeInfo = getCommentTypeLabel(comment.type);

    const isReply = !!comment.parentId;
    const actionText = isReply ? "replied to a comment" : `added a ${typeInfo.label.toLowerCase()}`;

    // Truncate and format comment content for display
    const maxLength = 800;
    const truncatedContent = comment.content.length > maxLength
      ? comment.content.substring(0, maxLength) + "…"
      : comment.content;

    const blocks: SlackBlock[] = [
      // Header with author and action
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `${typeInfo.emoji} ${authorDisplay} ${actionText} on <${bepUrl}|*BEP-${bep.number}: ${bep.title}*>`,
        },
      },
      // Comment content in a quote block
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `>${truncatedContent.split('\n').join('\n>')}`,
        },
      },
      // Context line with type badge
      {
        type: "context",
        elements: [
          {
            type: "mrkdwn",
            text: `${typeInfo.emoji} *${typeInfo.label}*`,
          },
          {
            type: "mrkdwn",
            text: `|`,
          },
          {
            type: "mrkdwn",
            text: `<!date^${Math.floor(comment.createdAt / 1000)}^{date_short_pretty} at {time}|${new Date(comment.createdAt).toISOString()}>`,
          },
        ],
      },
      // Action button
      {
        type: "actions",
        block_id: `comment_actions_${args.commentId}`,
        elements: [
          {
            type: "button",
            text: {
              type: "plain_text",
              text: "View & Reply",
              emoji: true,
            },
            url: commentPermalink,
            action_id: "view_comment",
            style: "primary",
          },
        ],
      },
    ];

    const result = await postToSlack(
      blocks,
      `${comment.authorName} commented on BEP-${bep.number}: ${truncatedContent.substring(0, 100)}`,
      bep.slackThreadTs
    );

    if (!bep.slackThreadTs && result.ok && result.ts) {
      await ctx.runMutation(internal.beps.storeSlackThreadTs, {
        bepId: args.bepId,
        slackThreadTs: result.ts,
      });
    }

    // Update the thread overview with new commenter info
    await ctx.runAction(internal.slack.updateBepThreadOverview, { bepId: args.bepId });
  },
});

/**
 * Notify Slack when a BEP's status changes.
 * Replies to the existing thread if one exists.
 * Also updates the thread overview message.
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

    // Update the thread overview with new status
    await ctx.runAction(internal.slack.updateBepThreadOverview, { bepId: args.bepId });
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
