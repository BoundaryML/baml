import { query } from "./_generated/server";
import { v } from "convex/values";
import { Id } from "./_generated/dataModel";

// ─────────────────────────────────────────────────────────────────────────────
// EXPORT QUERY - Compiles all BEP data for download
// ─────────────────────────────────────────────────────────────────────────────

export const getFullBepForExport = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const bep = await ctx.db.get(args.bepId);
    if (!bep) return null;

    // Fetch all related data in parallel
    const [
      pages,
      comments,
      decisions,
      issues,
      versions,
      shepherds,
      summaries,
    ] = await Promise.all([
      // Pages ordered by order field
      ctx.db
        .query("bepPages")
        .withIndex("by_bep_order", (q) => q.eq("bepId", args.bepId))
        .collect(),

      // All comments with author info
      ctx.db
        .query("comments")
        .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
        .collect(),

      // All decisions
      ctx.db
        .query("decisions")
        .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
        .collect(),

      // All issues
      ctx.db
        .query("openIssues")
        .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
        .collect(),

      // All versions (full history)
      ctx.db
        .query("bepVersions")
        .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
        .order("desc")
        .collect(),

      // Shepherd info
      Promise.all(bep.shepherds.map((id) => ctx.db.get(id))),

      // AI summaries
      ctx.db
        .query("summaries")
        .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
        .collect(),
    ]);

    // Collect all user IDs we need to resolve
    const userIds = new Set<Id<"users">>();

    // From comments
    for (const comment of comments) {
      userIds.add(comment.authorId);
      if (comment.resolvedBy) userIds.add(comment.resolvedBy);
    }

    // From decisions
    for (const decision of decisions) {
      for (const participantId of decision.participants) {
        userIds.add(participantId);
      }
    }

    // From issues
    for (const issue of issues) {
      userIds.add(issue.raisedBy);
      if (issue.assignedTo) userIds.add(issue.assignedTo);
    }

    // From versions
    for (const version of versions) {
      userIds.add(version.editedBy);
    }

    // From summaries
    for (const summary of summaries) {
      if (summary.reviewedBy) userIds.add(summary.reviewedBy);
    }

    // Fetch all users at once using properly typed IDs
    const userIdArray = Array.from(userIds);
    const users = await Promise.all(
      userIdArray.map((id) => ctx.db.get(id))
    );

    // Create user lookup map
    const userMap: Record<string, { name: string; avatarUrl?: string }> = {};
    for (let i = 0; i < userIdArray.length; i++) {
      const user = users[i];
      if (user) {
        userMap[userIdArray[i]] = { name: user.name, avatarUrl: user.avatarUrl };
      }
    }

    // Create version lookup map for getting version numbers
    const versionMap: Record<string, number> = {};
    for (const version of versions) {
      versionMap[version._id] = version.version;
    }

    // Get current version number (highest version)
    const currentVersionNumber = versions.length > 0 ? versions[0].version : 1;

    // Enrich comments with author names and version numbers
    const enrichedComments = comments.map((comment) => ({
      ...comment,
      authorName: userMap[comment.authorId]?.name ?? "Unknown",
      resolvedByName: comment.resolvedBy
        ? userMap[comment.resolvedBy]?.name
        : undefined,
      // Add version number for the comment (for determining OUTDATED status)
      versionNumber: comment.versionId ? versionMap[comment.versionId] : undefined,
    }));

    // Enrich decisions with participant names
    const enrichedDecisions = decisions.map((decision) => ({
      ...decision,
      participantNames: decision.participants
        .map((id) => userMap[id]?.name ?? "Unknown")
        .filter(Boolean),
    }));

    // Enrich issues with user names
    const enrichedIssues = issues.map((issue) => ({
      ...issue,
      raisedByName: userMap[issue.raisedBy]?.name ?? "Unknown",
      assignedToName: issue.assignedTo
        ? userMap[issue.assignedTo]?.name
        : undefined,
    }));

    // Enrich versions with editor names
    const enrichedVersions = versions.map((version) => ({
      ...version,
      editorName: userMap[version.editedBy]?.name ?? "Unknown",
    }));

    // Enrich summaries with reviewer names
    const enrichedSummaries = summaries.map((summary) => ({
      ...summary,
      reviewerName: summary.reviewedBy
        ? userMap[summary.reviewedBy]?.name
        : undefined,
    }));

    return {
      bep: {
        ...bep,
        shepherdNames: shepherds.filter((s) => s !== null).map((s) => s!.name),
      },
      pages,
      comments: enrichedComments,
      decisions: enrichedDecisions,
      issues: enrichedIssues,
      versions: enrichedVersions,
      summaries: enrichedSummaries,
      currentVersion: currentVersionNumber,
      exportedAt: Date.now(),
    };
  },
});
