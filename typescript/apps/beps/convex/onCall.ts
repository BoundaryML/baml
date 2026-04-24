"use node";

import { query, internalMutation, internalQuery, internalAction } from "./_generated/server";
import { v } from "convex/values";
import { internal } from "./_generated/api";
import { Id } from "./_generated/dataModel";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Get the current on-call rotation info.
 * Returns the current on-call user with their details.
 */
export const getCurrent = query({
  args: {},
  handler: async (ctx) => {
    // Get the most recent on-call rotation entry
    const rotations = await ctx.db
      .query("onCallRotation")
      .order("desc")
      .take(1);

    if (rotations.length === 0) {
      return null;
    }

    const rotation = rotations[0];
    
    // Get current user details
    const currentUser = await ctx.db.get(rotation.currentUserId);
    
    // Get next user details if available
    let nextUser = null;
    if (rotation.nextUserId) {
      nextUser = await ctx.db.get(rotation.nextUserId);
    }

    return {
      ...rotation,
      currentUser: currentUser ? {
        _id: currentUser._id,
        name: currentUser.name,
        avatarUrl: currentUser.avatarUrl,
        slackUserId: currentUser.slackUserId,
      } : null,
      nextUser: nextUser ? {
        _id: nextUser._id,
        name: nextUser.name,
        avatarUrl: nextUser.avatarUrl,
      } : null,
    };
  },
});

/**
 * Get on-call rotation by year and week.
 */
export const getByYearWeek = query({
  args: {
    year: v.number(),
    weekNumber: v.number(),
  },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("onCallRotation")
      .withIndex("by_year_week", (q) =>
        q.eq("year", args.year).eq("weekNumber", args.weekNumber)
      )
      .unique();
  },
});

// Internal query for use in HTTP handlers
export const getCurrentInternal = internalQuery({
  args: {},
  handler: async (ctx) => {
    const rotations = await ctx.db
      .query("onCallRotation")
      .order("desc")
      .take(1);

    if (rotations.length === 0) {
      return null;
    }

    return rotations[0];
  },
});

/**
 * Get all team members eligible for on-call rotation.
 * Returns users with team or bdfl roles (the BAML team).
 */
export const getRotationEligibleUsers = query({
  args: {},
  handler: async (ctx) => {
    const users = await ctx.db.query("users").collect();
    
    // Filter to team members (bdfl, team, or legacy admin, shepherd)
    // and special accounts (isSpecialAccount = true)
    return users
      .filter((user) => {
        const hasTeamRole = 
          user.role === "bdfl" || 
          user.role === "team" || 
          user.role === "admin" || 
          user.role === "shepherd";
        return hasTeamRole || user.isSpecialAccount;
      })
      .map((user) => ({
        _id: user._id,
        name: user.name,
        avatarUrl: user.avatarUrl,
        slackUserId: user.slackUserId,
        role: user.role,
      }));
  },
});

/**
 * Internal version of getRotationEligibleUsers for use in HTTP handlers.
 */
export const getRotationEligibleUsersInternal = internalQuery({
  args: {},
  handler: async (ctx) => {
    const users = await ctx.db.query("users").collect();
    
    return users
      .filter((user) => {
        const hasTeamRole = 
          user.role === "bdfl" || 
          user.role === "team" || 
          user.role === "admin" || 
          user.role === "shepherd";
        return hasTeamRole || user.isSpecialAccount;
      })
      .map((user) => ({
        _id: user._id,
        name: user.name,
        avatarUrl: user.avatarUrl,
        slackUserId: user.slackUserId,
        role: user.role,
      }));
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Internal mutation for updating rotation by username (used by HTTP endpoint).
 */
export const updateRotationByName = internalMutation({
  args: {
    currentUserName: v.string(),
    nextUserName: v.optional(v.string()),
    weekNumber: v.number(),
    year: v.number(),
    rotationOrder: v.array(v.string()),
    currentIndex: v.number(),
  },
  handler: async (ctx, args) => {
    // Look up users by name
    const currentUser = await ctx.db
      .query("users")
      .withIndex("by_name", (q) => q.eq("name", args.currentUserName))
      .unique();

    if (!currentUser) {
      throw new Error(`User not found: ${args.currentUserName}`);
    }

    let nextUser = null;
    if (args.nextUserName) {
      nextUser = await ctx.db
        .query("users")
        .withIndex("by_name", (q) => q.eq("name", args.nextUserName))
        .unique();
    }

    // Look up all users in rotation order
    const rotationUserIds: Id<"users">[] = [];
    for (const userName of args.rotationOrder) {
      const user = await ctx.db
        .query("users")
        .withIndex("by_name", (q) => q.eq("name", userName))
        .unique();
      if (user) {
        rotationUserIds.push(user._id);
      }
    }

    // Calculate week start and end times
    const now = Date.now();
    const weekInMs = 7 * 24 * 60 * 60 * 1000;

    // Check if rotation already exists for this week
    const existing = await ctx.db
      .query("onCallRotation")
      .withIndex("by_year_week", (q) =>
        q.eq("year", args.year).eq("weekNumber", args.weekNumber)
      )
      .unique();

    if (existing) {
      // Update existing rotation
      await ctx.db.patch(existing._id, {
        currentUserId: currentUser._id,
        nextUserId: nextUser?._id,
        rotationOrder: rotationUserIds,
        currentIndex: args.currentIndex,
        updatedAt: now,
      });
      return { updated: true, rotationId: existing._id };
    }

    // Create new rotation entry
    const rotationId = await ctx.db.insert("onCallRotation", {
      currentUserId: currentUser._id,
      nextUserId: nextUser?._id,
      weekNumber: args.weekNumber,
      year: args.year,
      rotationOrder: rotationUserIds,
      currentIndex: args.currentIndex,
      startedAt: now,
      endsAt: now + weekInMs,
      updatedAt: now,
    });

    return { updated: false, rotationId };
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// ACTIONS
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Notify Slack #general channel about on-call rotation change.
 */
export const notifySlackOnCallChange = internalAction({
  args: {
    currentUserName: v.string(),
    nextUserName: v.optional(v.string()),
    weekNumber: v.number(),
    year: v.number(),
  },
  handler: async (ctx, args) => {
    const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;
    const channel = process.env.SLACK_GENERAL_CHANNEL || "#general";

    if (!token) {
      console.warn("SLACK_BOUNDARY_BOT_TOKEN not configured - skipping on-call notification");
      return;
    }

    // Look up current user to get their Slack ID
    const currentUser = await ctx.runQuery(internal.users.getByNameInternal, {
      name: args.currentUserName,
    });

    const currentMention = currentUser?.slackUserId
      ? `<@${currentUser.slackUserId}>`
      : `*${args.currentUserName}*`;

    const nextMention = args.nextUserName ? `@${args.nextUserName}` : "TBD";

    // Get BEPs site URL
    const bepsUrl = process.env.NEXT_PUBLIC_APP_URL || "https://beps.boundaryml.com";

    const blocks = [
      {
        type: "header",
        text: {
          type: "plain_text",
          text: ":rotating_light: On-Call Rotation Update",
          emoji: true,
        },
      },
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: "Hey team! It's time for the weekly on-call rotation update.",
        },
      },
      {
        type: "divider",
      },
      {
        type: "section",
        fields: [
          {
            type: "mrkdwn",
            text: `*:calendar: Week:*\n${args.weekNumber} of ${args.year}`,
          },
        ],
      },
      {
        type: "section",
        text: {
          type: "mrkdwn",
          text: `:point_right: *This week's on-call:* ${currentMention}`,
        },
      },
      {
        type: "context",
        elements: [
          {
            type: "mrkdwn",
            text: `:crystal_ball: Next week: ${nextMention}`,
          },
        ],
      },
      {
        type: "divider",
      },
      {
        type: "context",
        elements: [
          {
            type: "mrkdwn",
            text: `View on-call status at <${bepsUrl}|BEPs site>`,
          },
        ],
      },
    ];

    try {
      const response = await fetch("https://slack.com/api/chat.postMessage", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify({
          channel,
          blocks,
          text: `On-Call Rotation: ${args.currentUserName} is on-call for week ${args.weekNumber}`,
        }),
      });

      const result = (await response.json()) as { ok: boolean; error?: string };
      if (!result.ok) {
        console.error("Slack API error:", result.error);
      }
    } catch (error) {
      console.error("Failed to post to Slack:", error);
    }
  },
});
