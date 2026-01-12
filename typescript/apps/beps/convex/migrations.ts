import { mutation } from "./_generated/server";

/**
 * Migration: Content Model Refactor (Phase 9)
 *
 * This migration updates existing data from the old 4-field section model
 * (summary, motivation, proposal, alternatives) to the new single `content` field model.
 *
 * Run this migration ONCE after deploying the schema changes:
 *   npx convex run migrations:migrateToContentModel
 */
export const migrateToContentModel = mutation({
  args: {},
  handler: async (ctx) => {
    const results = {
      bepsMigrated: 0,
      bepVersionsMigrated: 0,
      errors: [] as string[],
    };

    // Migrate BEPs
    const beps = await ctx.db.query("beps").collect();

    for (const bep of beps) {
      try {
        // Check if already migrated (has content field)
        if ("content" in bep && typeof bep.content === "string") {
          continue;
        }

        // Build content from old fields
        const oldBep = bep as unknown as {
          _id: typeof bep._id;
          summary?: string;
          motivation?: string;
          proposal?: string;
          alternatives?: string;
        };

        const sections: string[] = [];

        if (oldBep.summary) {
          sections.push(`## Summary\n\n${oldBep.summary}`);
        }
        if (oldBep.motivation) {
          sections.push(`## Motivation\n\n${oldBep.motivation}`);
        }
        if (oldBep.proposal) {
          sections.push(`## Proposal\n\n${oldBep.proposal}`);
        }
        if (oldBep.alternatives) {
          sections.push(`## Alternatives\n\n${oldBep.alternatives}`);
        }

        const content = sections.join("\n\n");

        // Update the BEP with the new content field
        await ctx.db.patch(bep._id, {
          content,
          // Remove old fields by setting them to undefined (Convex will delete them)
        });

        results.bepsMigrated++;
      } catch (error) {
        results.errors.push(`BEP ${bep._id}: ${String(error)}`);
      }
    }

    // Migrate BEP Versions
    const versions = await ctx.db.query("bepVersions").collect();

    for (const version of versions) {
      try {
        // Check if already migrated
        if ("content" in version && typeof version.content === "string") {
          continue;
        }

        // Build content from old fields
        const oldVersion = version as unknown as {
          _id: typeof version._id;
          summary?: string;
          motivation?: string;
          proposal?: string;
          alternatives?: string;
        };

        const sections: string[] = [];

        if (oldVersion.summary) {
          sections.push(`## Summary\n\n${oldVersion.summary}`);
        }
        if (oldVersion.motivation) {
          sections.push(`## Motivation\n\n${oldVersion.motivation}`);
        }
        if (oldVersion.proposal) {
          sections.push(`## Proposal\n\n${oldVersion.proposal}`);
        }
        if (oldVersion.alternatives) {
          sections.push(`## Alternatives\n\n${oldVersion.alternatives}`);
        }

        const content = sections.join("\n\n");

        // Update the version with the new content field
        await ctx.db.patch(version._id, {
          content,
        });

        results.bepVersionsMigrated++;
      } catch (error) {
        results.errors.push(`BEPVersion ${version._id}: ${String(error)}`);
      }
    }

    // Migrate comments from sectionId/sectionSlug to pageId
    // Since we're removing sections entirely, existing section comments
    // will be associated with the main content (pageId = undefined)
    const comments = await ctx.db.query("comments").collect();

    let commentsMigrated = 0;
    for (const comment of comments) {
      try {
        const oldComment = comment as unknown as {
          _id: typeof comment._id;
          sectionId?: string;
          sectionSlug?: string;
        };

        // If comment has old sectionId/sectionSlug fields, clear them
        // The pageId field will default to undefined (main content)
        if (oldComment.sectionId || oldComment.sectionSlug) {
          await ctx.db.patch(comment._id, {
            // pageId will be undefined, associating with main content
          });
          commentsMigrated++;
        }
      } catch (error) {
        results.errors.push(`Comment ${comment._id}: ${String(error)}`);
      }
    }

    return {
      ...results,
      commentsMigrated,
      message: `Migration complete. Migrated ${results.bepsMigrated} BEPs, ${results.bepVersionsMigrated} versions, ${commentsMigrated} comments.`,
    };
  },
});

/**
 * Migration: Assign Version IDs to Comments (Phase 10)
 *
 * This migration assigns existing comments to their BEP's latest version.
 * Comments without a versionId will be assigned to the current (latest) version.
 *
 * Run this migration ONCE after deploying the schema changes:
 *   npx convex run migrations:migrateCommentsToVersions
 */
export const migrateCommentsToVersions = mutation({
  args: {},
  handler: async (ctx) => {
    const results = {
      commentsMigrated: 0,
      commentsSkipped: 0,
      errors: [] as string[],
    };

    // Get all comments
    const comments = await ctx.db.query("comments").collect();

    // Group comments by bepId for efficiency
    const commentsByBep = new Map<string, typeof comments>();
    for (const comment of comments) {
      const bepIdStr = comment.bepId.toString();
      if (!commentsByBep.has(bepIdStr)) {
        commentsByBep.set(bepIdStr, []);
      }
      commentsByBep.get(bepIdStr)!.push(comment);
    }

    // For each BEP, get the latest version and assign it to comments without versionId
    for (const [bepIdStr, bepComments] of commentsByBep) {
      try {
        // Get the first comment to extract the actual bepId
        const sampleComment = bepComments[0];
        if (!sampleComment) continue;

        // Get the latest version for this BEP
        const latestVersion = await ctx.db
          .query("bepVersions")
          .withIndex("by_bep", (q) => q.eq("bepId", sampleComment.bepId))
          .order("desc")
          .first();

        if (!latestVersion) {
          results.errors.push(`BEP ${bepIdStr}: No versions found`);
          continue;
        }

        // Update all comments for this BEP that don't have a versionId
        for (const comment of bepComments) {
          if (comment.versionId) {
            results.commentsSkipped++;
            continue;
          }

          await ctx.db.patch(comment._id, {
            versionId: latestVersion._id,
          });
          results.commentsMigrated++;
        }
      } catch (error) {
        results.errors.push(`BEP ${bepIdStr}: ${String(error)}`);
      }
    }

    return {
      ...results,
      message: `Migration complete. Migrated ${results.commentsMigrated} comments, skipped ${results.commentsSkipped} (already had versionId).`,
    };
  },
});
