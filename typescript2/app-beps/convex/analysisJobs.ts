import { v } from "convex/values";
import { mutation, query, internalMutation, internalQuery } from "./_generated/server";
import { internal } from "./_generated/api";
import { Id } from "./_generated/dataModel";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Get the latest analysis job for a specific version
 */
export const getByVersion = query({
  args: { versionId: v.id("bepVersions") },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("versionAnalysisJobs")
      .withIndex("by_version", (q) => q.eq("versionId", args.versionId))
      .order("desc")
      .first();
  },
});

/**
 * Get all analysis jobs for a BEP
 */
export const getByBep = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("versionAnalysisJobs")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .order("desc")
      .collect();
  },
});

/**
 * Internal query to get job by ID
 */
export const getById = internalQuery({
  args: { jobId: v.id("versionAnalysisJobs") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.jobId);
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Create a new analysis job and schedule the background analysis
 */
export const create = internalMutation({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
    previousVersionId: v.id("bepVersions"),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    // Create the job record
    const jobId = await ctx.db.insert("versionAnalysisJobs", {
      bepId: args.bepId,
      versionId: args.versionId,
      previousVersionId: args.previousVersionId,
      status: "pending",
      createdAt: now,
    });

    // Schedule the background analysis (Node.js action for BAML support)
    await ctx.scheduler.runAfter(0, internal.analysisJobsNode.runAnalysis, {
      jobId,
    });

    return jobId;
  },
});

/**
 * Internal mutation to update job status
 */
export const updateStatus = internalMutation({
  args: {
    jobId: v.id("versionAnalysisJobs"),
    status: v.union(
      v.literal("pending"),
      v.literal("analyzing"),
      v.literal("completed"),
      v.literal("failed")
    ),
    result: v.optional(v.any()),
    error: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const now = Date.now();
    const updates: Record<string, unknown> = { status: args.status };

    if (args.status === "analyzing") {
      updates.startedAt = now;
    } else if (args.status === "completed" || args.status === "failed") {
      updates.completedAt = now;
    }

    if (args.result !== undefined) {
      updates.result = args.result;
    }

    if (args.error !== undefined) {
      updates.error = args.error;
    }

    await ctx.db.patch(args.jobId, updates);
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// INTERNAL QUERIES FOR ANALYSIS
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Get all context needed for analysis
 */
export const getAnalysisContext = internalQuery({
  args: { jobId: v.id("versionAnalysisJobs") },
  handler: async (ctx, args) => {
    const job = await ctx.db.get(args.jobId);
    if (!job) throw new Error("Job not found");

    // Get BEP
    const bep = await ctx.db.get(job.bepId);
    if (!bep) throw new Error("BEP not found");

    // Get versions
    const newVersion = await ctx.db.get(job.versionId);
    const previousVersion = await ctx.db.get(job.previousVersionId);
    if (!newVersion || !previousVersion) throw new Error("Versions not found");

    // Get editor names
    const [newEditor, prevEditor] = await Promise.all([
      ctx.db.get(newVersion.editedBy),
      ctx.db.get(previousVersion.editedBy),
    ]);

    // Get comments from previous version (focus on unresolved)
    const comments = await ctx.db
      .query("comments")
      .withIndex("by_bep_version", (q) =>
        q.eq("bepId", job.bepId).eq("versionId", job.previousVersionId)
      )
      .collect();

    // Get open issues
    const issues = await ctx.db
      .query("openIssues")
      .withIndex("by_bep", (q) => q.eq("bepId", job.bepId))
      .collect();

    // Get decisions
    const decisions = await ctx.db
      .query("decisions")
      .withIndex("by_bep", (q) => q.eq("bepId", job.bepId))
      .collect();

    // Get author names for comments and issues
    const userIds = new Set<Id<"users">>();
    comments.forEach(c => userIds.add(c.authorId));
    issues.forEach(i => userIds.add(i.raisedBy));

    const users = await Promise.all(
      Array.from(userIds).map(id => ctx.db.get(id))
    );
    const userMap = new Map(users.filter(u => u).map(u => [u!._id, u!.name]));

    return {
      bep: {
        number: bep.number,
        title: bep.title,
      },
      newVersion: {
        version: newVersion.version,
        title: newVersion.title,
        content: newVersion.content ?? "",
        editedBy: newEditor?.name ?? "Unknown",
        editNote: newVersion.editNote,
      },
      previousVersion: {
        version: previousVersion.version,
        title: previousVersion.title,
        content: previousVersion.content ?? "",
        editedBy: prevEditor?.name ?? "Unknown",
        editNote: previousVersion.editNote,
      },
      comments: comments.map(c => ({
        id: c._id,
        type: c.type,
        content: c.content,
        author: userMap.get(c.authorId) ?? "Unknown",
        resolved: c.resolved,
        resolution: undefined, // Comments don't have resolution field
      })),
      issues: issues.map(i => ({
        id: i._id,
        type: "issue",
        content: `${i.title}${i.description ? `: ${i.description}` : ""}`,
        author: userMap.get(i.raisedBy) ?? "Unknown",
        resolved: i.resolved,
        resolution: i.resolution,
      })),
      decisions: decisions.map(d => ({
        id: d._id,
        type: "decision",
        content: `${d.title}: ${d.description}${d.rationale ? ` (Rationale: ${d.rationale})` : ""}`,
        author: "Team", // Decisions are made by participants
        resolved: true, // Decisions are inherently "resolved"
        resolution: undefined,
      })),
    };
  },
});

// Note: runAnalysis is in analysisJobsNode.ts (Node.js action with BAML)

/**
 * Retry a failed analysis job
 */
export const retry = mutation({
  args: { jobId: v.id("versionAnalysisJobs") },
  handler: async (ctx, args) => {
    const job = await ctx.db.get(args.jobId);
    if (!job) throw new Error("Job not found");
    if (job.status !== "failed") throw new Error("Can only retry failed jobs");

    // Reset job status
    await ctx.db.patch(args.jobId, {
      status: "pending",
      error: undefined,
      startedAt: undefined,
      completedAt: undefined,
    });

    // Re-schedule the analysis (Node.js action for BAML support)
    await ctx.scheduler.runAfter(0, internal.analysisJobsNode.runAnalysis, {
      jobId: args.jobId,
    });
  },
});
