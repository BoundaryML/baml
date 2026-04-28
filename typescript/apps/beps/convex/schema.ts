import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

// ─────────────────────────────────────────────────────────────────────────────
// VALIDATORS (reusable)
// ─────────────────────────────────────────────────────────────────────────────

export const bepStatus = v.union(
  v.literal("draft"),
  v.literal("proposed"),
  v.literal("pending"),
  v.literal("accepted"),
  v.literal("implemented"),
  v.literal("rejected"),
  v.literal("superseded")
);

export const userStance = v.union(
  v.literal("support"),
  v.literal("neutral"),
  v.literal("oppose")
);

export const commentType = v.union(
  v.literal("discussion"),    // General feedback
  v.literal("concern"),       // Blocking issue
  v.literal("question")       // Needs clarification
);

export const summaryStatus = v.union(
  v.literal("draft"),         // AI-generated, not reviewed
  v.literal("approved"),      // Human-approved
  v.literal("applied")        // Incorporated into BEP
);

export const userRole = v.union(
  // New roles
  v.literal("bdfl"),
  v.literal("team"),
  v.literal("unset"),
  // Legacy roles (kept for backwards compatibility with existing data)
  // Run `npx convex run migrations:migrateUserRoles` to migrate to new roles
  v.literal("admin"),
  v.literal("shepherd"),
  v.literal("member")
);

// ─────────────────────────────────────────────────────────────────────────────
// SCHEMA
// ─────────────────────────────────────────────────────────────────────────────

export default defineSchema({
  // ─────────────────────────────────────────────────────────────────────────
  // USERS (GitHub OAuth or passkey auth)
  // ─────────────────────────────────────────────────────────────────────────
  users: defineTable({
    name: v.string(),
    avatarUrl: v.optional(v.string()),
    githubId: v.optional(v.string()),
    role: userRole,
    createdAt: v.number(),
    // Special account fields for BoundaryML team members
    isSpecialAccount: v.optional(v.boolean()),
    boundaryEmail: v.optional(v.string()),
    slackUserId: v.optional(v.string()),
    // API token for agent access
    apiToken: v.optional(v.string()),
  })
    .index("by_name", ["name"])
    .index("by_github_id", ["githubId"])
    .index("by_api_token", ["apiToken"]),

  // ─────────────────────────────────────────────────────────────────────────
  // BEPs (main proposals)
  // ─────────────────────────────────────────────────────────────────────────
  beps: defineTable({
    number: v.number(),                                // BEP-001, BEP-002, etc.
    title: v.string(),
    status: bepStatus,
    shepherds: v.array(v.id("users")),

    // Content (markdown) - main proposal content, becomes README.md on export
    // NOTE: Optional during migration, will be required after migration completes
    content: v.optional(v.string()),

    // Legacy fields (will be removed after migration)
    summary: v.optional(v.string()),
    motivation: v.optional(v.string()),
    proposal: v.optional(v.string()),
    alternatives: v.optional(v.string()),

    // Metadata
    createdAt: v.number(),
    updatedAt: v.number(),
    supersededBy: v.optional(v.id("beps")),

    // Who implemented this BEP (optional, set when status becomes implemented)
    implementedBy: v.optional(v.array(v.id("users"))),

    // Slack integration - thread_ts for #beps channel notifications
    slackThreadTs: v.optional(v.string()),

    // Good reference flag - marks this BEP as a good example for writing style
    isGoodReference: v.optional(v.boolean()),
  })
    .index("by_number", ["number"])
    .index("by_status", ["status"])
    .index("by_updatedAt", ["updatedAt"]),

  // ─────────────────────────────────────────────────────────────────────────
  // BEP VERSIONS (content history for versioning)
  // ─────────────────────────────────────────────────────────────────────────
  bepVersions: defineTable({
    bepId: v.id("beps"),
    version: v.number(),                               // 1, 2, 3, etc.

    // Snapshot of content at this version
    title: v.string(),
    // NOTE: Optional during migration, will be required after migration completes
    content: v.optional(v.string()),                   // Main content snapshot

    // Snapshot of pages at this version (array of page snapshots)
    pagesSnapshot: v.optional(v.array(v.object({
      slug: v.string(),
      title: v.string(),
      content: v.string(),
      order: v.number(),
    }))),

    // Legacy fields (will be removed after migration)
    summary: v.optional(v.string()),
    motivation: v.optional(v.string()),
    proposal: v.optional(v.string()),
    alternatives: v.optional(v.string()),

    // Who made this version
    editedBy: v.id("users"),
    editNote: v.optional(v.string()),                  // Optional change description
    createdAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_bep_version", ["bepId", "version"]),

  // ─────────────────────────────────────────────────────────────────────────
  // BEP PAGES (additional pages like "background", "tooling")
  // Think of it like a wiki with a main page (content field) and subpages
  // ─────────────────────────────────────────────────────────────────────────
  bepPages: defineTable({
    bepId: v.id("beps"),
    slug: v.string(),                                  // URL-friendly identifier, e.g., "background", "tooling"
    title: v.string(),
    content: v.string(),
    order: v.number(),
    createdAt: v.number(),
    updatedAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_bep_order", ["bepId", "order"])
    .index("by_bep_slug", ["bepId", "slug"]),

  // ─────────────────────────────────────────────────────────────────────────
  // COMMENTS
  // ─────────────────────────────────────────────────────────────────────────
  comments: defineTable({
    bepId: v.id("beps"),
    versionId: v.optional(v.id("bepVersions")),        // Which version this comment is on (optional during migration)
    pageId: v.optional(v.id("bepPages")),              // Which page (null = main content)
    authorId: v.id("users"),
    parentId: v.optional(v.id("comments")),            // For threading

    // Legacy fields (will be removed after migration)
    sectionId: v.optional(v.string()),
    sectionSlug: v.optional(v.string()),

    type: commentType,
    content: v.string(),

    // For inline comments - block-level positioning (Tiptap node-based)
    anchor: v.optional(v.object({
      nodeId: v.string(),                              // Tiptap node ID
      nodeType: v.string(),                            // Node type (paragraph, heading, etc.)
      nodeText: v.string(),                            // Preview text for context
    })),

    // Reactions { emoji: [userId, userId, ...] }
    reactions: v.optional(v.object({
      thumbsUp: v.optional(v.array(v.id("users"))),
      thumbsDown: v.optional(v.array(v.id("users"))),
      heart: v.optional(v.array(v.id("users"))),
      thinking: v.optional(v.array(v.id("users"))),
    })),

    // Resolution
    resolved: v.boolean(),
    resolvedBy: v.optional(v.id("users")),
    resolvedAt: v.optional(v.number()),

    createdAt: v.number(),
    updatedAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_bep_page", ["bepId", "pageId"])
    .index("by_version", ["versionId"])
    .index("by_bep_version", ["bepId", "versionId"])
    .index("by_parent", ["parentId"])
    .index("by_bep_unresolved", ["bepId", "resolved"]),

  // ─────────────────────────────────────────────────────────────────────────
  // DECISIONS
  // ─────────────────────────────────────────────────────────────────────────
  decisions: defineTable({
    bepId: v.id("beps"),

    title: v.string(),
    description: v.string(),
    rationale: v.optional(v.string()),

    // Link to source comments
    sourceCommentIds: v.array(v.id("comments")),

    // Participants in the decision
    participants: v.array(v.id("users")),

    decidedAt: v.number(),
    createdAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_decidedAt", ["decidedAt"]),

  // ─────────────────────────────────────────────────────────────────────────
  // OPEN ISSUES
  // ─────────────────────────────────────────────────────────────────────────
  openIssues: defineTable({
    bepId: v.id("beps"),

    title: v.string(),
    description: v.optional(v.string()),
    raisedBy: v.id("users"),
    assignedTo: v.optional(v.id("users")),

    // Link to source comment (legacy, kept for backwards compatibility)
    sourceCommentId: v.optional(v.id("comments")),
    // Related comments (for attaching multiple comments)
    relatedCommentIds: v.optional(v.array(v.id("comments"))),

    // Resolution
    resolved: v.boolean(),
    resolution: v.optional(v.string()),
    resolvedAt: v.optional(v.number()),

    createdAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_bep_unresolved", ["bepId", "resolved"])
    .index("by_assignee", ["assignedTo"]),

  // ─────────────────────────────────────────────────────────────────────────
  // PRESENCE (who's viewing a BEP)
  // ─────────────────────────────────────────────────────────────────────────
  presence: defineTable({
    bepId: v.id("beps"),
    userId: v.id("users"),
    lastSeen: v.number(),                            // Timestamp of last heartbeat
  })
    .index("by_bep", ["bepId"])
    .index("by_user_bep", ["userId", "bepId"]),

  // ─────────────────────────────────────────────────────────────────────────
  // VERSION ANALYSIS JOBS (AI-powered feedback analysis)
  // ─────────────────────────────────────────────────────────────────────────
  versionAnalysisJobs: defineTable({
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),           // The new version being analyzed
    previousVersionId: v.id("bepVersions"),   // The version being compared against

    // Job status
    status: v.union(
      v.literal("pending"),
      v.literal("analyzing"),
      v.literal("completed"),
      v.literal("failed")
    ),

    // Analysis result (populated on completion)
    result: v.optional(v.object({
      addressedFeedback: v.array(v.object({
        feedbackId: v.string(),
        feedbackType: v.string(),
        originalContent: v.string(),
        status: v.string(),
        evidence: v.string(),
        explanation: v.string(),
      })),
      unaddressedFeedback: v.array(v.object({
        feedbackId: v.string(),
        feedbackType: v.string(),
        originalContent: v.string(),
        status: v.string(),
        evidence: v.string(),
        explanation: v.string(),
      })),
      partiallyAddressedFeedback: v.array(v.object({
        feedbackId: v.string(),
        feedbackType: v.string(),
        originalContent: v.string(),
        status: v.string(),
        evidence: v.string(),
        explanation: v.string(),
      })),
      overallVerdict: v.string(),
      verdictExplanation: v.string(),
      recommendations: v.array(v.object({
        priority: v.string(),
        description: v.string(),
        relatedFeedbackIds: v.array(v.string()),
      })),
      summary: v.string(),
    })),

    // Error info (populated on failure)
    error: v.optional(v.string()),

    // Timing
    createdAt: v.number(),
    startedAt: v.optional(v.number()),
    completedAt: v.optional(v.number()),
  })
    .index("by_bep", ["bepId"])
    .index("by_version", ["versionId"])
    .index("by_status", ["status"]),

  // ─────────────────────────────────────────────────────────────────────────
  // USER STANCES (personal status on BEP versions)
  // Tracks individual user opinions/acceptance for historical analysis
  // ─────────────────────────────────────────────────────────────────────────
  userStances: defineTable({
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),           // Which version this stance is for
    userId: v.id("users"),

    stance: userStance,                        // support, neutral, oppose
    comment: v.optional(v.string()),           // Optional brief comment explaining stance

    createdAt: v.number(),
    updatedAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_bep_version", ["bepId", "versionId"])
    .index("by_user", ["userId"])
    .index("by_bep_user", ["bepId", "userId"])
    .index("by_version_user", ["versionId", "userId"]),

  // ─────────────────────────────────────────────────────────────────────────
  // SUMMARIES (AI-generated)
  // ─────────────────────────────────────────────────────────────────────────
  summaries: defineTable({
    bepId: v.id("beps"),

    // What comments were summarized
    commentIds: v.array(v.id("comments")),
    periodStart: v.number(),
    periodEnd: v.number(),

    // AI-generated content
    content: v.string(),                               // Markdown summary
    themes: v.optional(v.array(v.object({
      name: v.string(),
      summary: v.string(),
      sentiment: v.union(v.literal("positive"), v.literal("neutral"), v.literal("concern")),
      commentIds: v.array(v.id("comments")),
    }))),
    suggestedUpdates: v.optional(v.array(v.object({
      target: v.string(),                              // e.g., "proposal", section slug
      action: v.union(v.literal("add"), v.literal("update"), v.literal("remove")),
      content: v.string(),
      rationale: v.string(),
    }))),

    // Review status
    status: summaryStatus,
    reviewedBy: v.optional(v.id("users")),
    reviewedAt: v.optional(v.number()),

    // AI metadata
    model: v.string(),                                 // e.g., "claude-sonnet-4-20250514"
    promptVersion: v.string(),

    createdAt: v.number(),
  })
    .index("by_bep", ["bepId"])
    .index("by_bep_status", ["bepId", "status"]),
});
