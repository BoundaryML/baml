import { query, mutation, internalQuery } from "./_generated/server";
import { v } from "convex/values";
import { bepStatus } from "./schema";
import { internal } from "./_generated/api";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES (real-time subscriptions)
// ─────────────────────────────────────────────────────────────────────────────

export const list = query({
  args: {
    status: v.optional(bepStatus),
    limit: v.optional(v.number()),
  },
  handler: async (ctx, args) => {
    let beps;

    if (args.status) {
      beps = await ctx.db
        .query("beps")
        .withIndex("by_status", (q) => q.eq("status", args.status!))
        .take(args.limit ?? 50);
    } else {
      beps = await ctx.db
        .query("beps")
        .withIndex("by_updatedAt")
        .order("desc")
        .take(args.limit ?? 50);
    }

    // Enrich with counts and shepherd info
    return Promise.all(
      beps.map(async (bep) => {
        const [commentCount, issueCount, shepherds] = await Promise.all([
          ctx.db
            .query("comments")
            .withIndex("by_bep", (q) => q.eq("bepId", bep._id))
            .collect()
            .then((c) => c.length),
          ctx.db
            .query("openIssues")
            .withIndex("by_bep_unresolved", (q) =>
              q.eq("bepId", bep._id).eq("resolved", false)
            )
            .collect()
            .then((i) => i.length),
          Promise.all(bep.shepherds.map((id) => ctx.db.get(id))),
        ]);
        return {
          ...bep,
          commentCount,
          openIssueCount: issueCount,
          shepherdNames: shepherds
            .filter((s) => s !== null)
            .map((s) => s!.name),
        };
      })
    );
  },
});

export const getByNumber = query({
  args: { number: v.number() },
  handler: async (ctx, args) => {
    const bep = await ctx.db
      .query("beps")
      .withIndex("by_number", (q) => q.eq("number", args.number))
      .unique();

    if (!bep) return null;

    // Get pages, comments, decisions, issues, versions, and shepherds
    const [pages, comments, decisions, issues, versions, shepherds] =
      await Promise.all([
        ctx.db
          .query("bepPages")
          .withIndex("by_bep_order", (q) => q.eq("bepId", bep._id))
          .collect(),
        ctx.db
          .query("comments")
          .withIndex("by_bep", (q) => q.eq("bepId", bep._id))
          .collect(),
        ctx.db
          .query("decisions")
          .withIndex("by_bep", (q) => q.eq("bepId", bep._id))
          .collect(),
        ctx.db
          .query("openIssues")
          .withIndex("by_bep", (q) => q.eq("bepId", bep._id))
          .collect(),
        ctx.db
          .query("bepVersions")
          .withIndex("by_bep", (q) => q.eq("bepId", bep._id))
          .order("desc")
          .take(10),
        Promise.all(bep.shepherds.map((id) => ctx.db.get(id))),
      ]);

    return {
      ...bep,
      pages,
      comments,
      decisions,
      issues,
      versions,
      shepherdNames: shepherds.filter((s) => s !== null).map((s) => s!.name),
    };
  },
});

export const getNextNumber = query({
  args: {},
  handler: async (ctx) => {
    const latestBep = await ctx.db
      .query("beps")
      .withIndex("by_number")
      .order("desc")
      .first();

    return (latestBep?.number ?? 0) + 1;
  },
});

// Internal query for actions to use
export const getById = internalQuery({
  args: { id: v.id("beps") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.id);
  },
});

// Internal query to get version by ID with editor name
export const getVersionById = internalQuery({
  args: { id: v.id("bepVersions") },
  handler: async (ctx, args) => {
    const version = await ctx.db.get(args.id);
    if (!version) return null;

    const editor = await ctx.db.get(version.editedBy);
    return {
      ...version,
      editedByName: editor?.name ?? "Unknown",
    };
  },
});

// Internal query to get all versions for a BEP
export const getVersionsByBep = internalQuery({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const versions = await ctx.db
      .query("bepVersions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .order("desc")
      .collect();

    return Promise.all(
      versions.map(async (version) => {
        const editor = await ctx.db.get(version.editedBy);
        return {
          ...version,
          editedByName: editor?.name ?? "Unknown",
        };
      })
    );
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS (writes)
// ─────────────────────────────────────────────────────────────────────────────

export const create = mutation({
  args: {
    number: v.number(),
    title: v.string(),
    shepherds: v.array(v.id("users")),
    content: v.optional(v.string()),
    pages: v.optional(
      v.array(
        v.object({
          slug: v.string(),
          title: v.string(),
          content: v.string(),
        })
      )
    ),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const now = Date.now();
    const content = args.content ?? "";
    const latestBep = await ctx.db
      .query("beps")
      .withIndex("by_number")
      .order("desc")
      .first();

    let bepNumber = Math.max(args.number, (latestBep?.number ?? 0) + 1);
    while (true) {
      const existing = await ctx.db
        .query("beps")
        .withIndex("by_number", (q) => q.eq("number", bepNumber))
        .unique();
      if (!existing) break;
      bepNumber += 1;
    }

    const bepId = await ctx.db.insert("beps", {
      number: bepNumber,
      title: args.title,
      status: "draft",
      shepherds: args.shepherds,
      content,
      createdAt: now,
      updatedAt: now,
    });

    // Create initial pages if provided
    const pagesSnapshot: Array<{
      slug: string;
      title: string;
      content: string;
      order: number;
    }> = [];

    if (args.pages && args.pages.length > 0) {
      for (let i = 0; i < args.pages.length; i++) {
        const page = args.pages[i];
        await ctx.db.insert("bepPages", {
          bepId,
          slug: page.slug,
          title: page.title,
          content: page.content,
          order: i,
          createdAt: now,
          updatedAt: now,
        });
        pagesSnapshot.push({
          slug: page.slug,
          title: page.title,
          content: page.content,
          order: i,
        });
      }
    }

    // Create initial version
    await ctx.db.insert("bepVersions", {
      bepId,
      version: 1,
      title: args.title,
      content,
      pagesSnapshot: pagesSnapshot.length > 0 ? pagesSnapshot : undefined,
      editedBy: args.userId,
      editNote: "Initial creation",
      createdAt: now,
    });

    return { bepId, number: bepNumber };
  },
});

export const update = mutation({
  args: {
    id: v.id("beps"),
    title: v.optional(v.string()),
    content: v.optional(v.string()),
    pages: v.optional(
      v.array(
        v.object({
          _id: v.optional(v.id("bepPages")),
          slug: v.string(),
          title: v.string(),
          content: v.string(),
          order: v.number(),
        })
      )
    ),
    userId: v.id("users"),
    editNote: v.optional(v.string()),
    versionMode: v.optional(v.union(v.literal("new"), v.literal("current"))),
  },
  handler: async (ctx, args) => {
    const bep = await ctx.db.get(args.id);
    if (!bep) throw new Error("BEP not found");

    const now = Date.now();
    const updates: Record<string, unknown> = { updatedAt: now };

    if (args.title !== undefined) updates.title = args.title;
    if (args.content !== undefined) updates.content = args.content;

    await ctx.db.patch(args.id, updates);

    // Handle pages if provided
    if (args.pages !== undefined) {
      // Get existing pages
      const existingPages = await ctx.db
        .query("bepPages")
        .withIndex("by_bep_order", (q) => q.eq("bepId", args.id))
        .collect();

      const existingPageIds = new Set(existingPages.map((p) => p._id));
      const providedPageIds = new Set(
        args.pages.filter((p) => p._id !== undefined).map((p) => p._id!)
      );

      // Delete pages that are no longer in the list
      for (const existingPage of existingPages) {
        if (!providedPageIds.has(existingPage._id)) {
          await ctx.db.delete(existingPage._id);
        }
      }

      // Update existing pages and create new ones
      for (const page of args.pages) {
        if (page._id && existingPageIds.has(page._id)) {
          // Update existing page
          await ctx.db.patch(page._id, {
            slug: page.slug,
            title: page.title,
            content: page.content,
            order: page.order,
            updatedAt: now,
          });
        } else {
          // Create new page
          await ctx.db.insert("bepPages", {
            bepId: args.id,
            slug: page.slug,
            title: page.title,
            content: page.content,
            order: page.order,
            createdAt: now,
            updatedAt: now,
          });
        }
      }
    }

    // Get current pages for version snapshot (after updates)
    const pages = await ctx.db
      .query("bepPages")
      .withIndex("by_bep_order", (q) => q.eq("bepId", args.id))
      .collect();

    const pagesSnapshot = pages.map((p) => ({
      slug: p.slug,
      title: p.title,
      content: p.content,
      order: p.order,
    }));

    // Get the latest version for this BEP
    const latestVersion = await ctx.db
      .query("bepVersions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.id))
      .order("desc")
      .first();

    // Apply changes to current version (in-place) when requested
    if (args.versionMode === "current" && latestVersion) {
      await ctx.db.patch(latestVersion._id, {
        title: args.title ?? bep.title,
        content: args.content ?? bep.content ?? "",
        pagesSnapshot: pagesSnapshot.length > 0 ? pagesSnapshot : undefined,
        editedBy: args.userId,
        editNote: args.editNote ?? latestVersion.editNote,
      });
      return;
    }

    // Default behavior: create a new version
    const newVersionNumber = (latestVersion?.version ?? 0) + 1;

    const newVersionId = await ctx.db.insert("bepVersions", {
      bepId: args.id,
      version: newVersionNumber,
      title: args.title ?? bep.title,
      content: args.content ?? bep.content ?? "",
      pagesSnapshot: pagesSnapshot.length > 0 ? pagesSnapshot : undefined,
      editedBy: args.userId,
      editNote: args.editNote,
      createdAt: now,
    });

    // Trigger version analysis if this is v2 or later
    if (newVersionNumber >= 2 && latestVersion) {
      await ctx.scheduler.runAfter(0, internal.analysisJobs.create, {
        bepId: args.id,
        versionId: newVersionId,
        previousVersionId: latestVersion._id,
      });
    }
  },
});

export const updateStatus = mutation({
  args: {
    id: v.id("beps"),
    status: bepStatus,
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, {
      status: args.status,
      updatedAt: Date.now(),
    });

    // TODO: Trigger Slack notification
    // await ctx.scheduler.runAfter(0, internal.notifications.sendSlack, {
    //   type: "status_change",
    //   bepId: args.id,
    //   newStatus: args.status,
    // });
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// PAGE MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const createPage = mutation({
  args: {
    bepId: v.id("beps"),
    slug: v.string(),
    title: v.string(),
    content: v.string(),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    // Get the highest order for this BEP's pages
    const existingPages = await ctx.db
      .query("bepPages")
      .withIndex("by_bep_order", (q) => q.eq("bepId", args.bepId))
      .collect();

    const maxOrder = existingPages.reduce(
      (max, p) => Math.max(max, p.order),
      -1
    );

    const pageId = await ctx.db.insert("bepPages", {
      bepId: args.bepId,
      slug: args.slug,
      title: args.title,
      content: args.content,
      order: maxOrder + 1,
      createdAt: now,
      updatedAt: now,
    });

    // Update BEP's updatedAt
    await ctx.db.patch(args.bepId, { updatedAt: now });

    return pageId;
  },
});

export const updatePage = mutation({
  args: {
    pageId: v.id("bepPages"),
    title: v.optional(v.string()),
    content: v.optional(v.string()),
    slug: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const page = await ctx.db.get(args.pageId);
    if (!page) throw new Error("Page not found");

    const now = Date.now();
    const updates: Record<string, unknown> = { updatedAt: now };

    if (args.title !== undefined) updates.title = args.title;
    if (args.content !== undefined) updates.content = args.content;
    if (args.slug !== undefined) updates.slug = args.slug;

    await ctx.db.patch(args.pageId, updates);

    // Update BEP's updatedAt
    await ctx.db.patch(page.bepId, { updatedAt: now });
  },
});

export const deletePage = mutation({
  args: {
    pageId: v.id("bepPages"),
  },
  handler: async (ctx, args) => {
    const page = await ctx.db.get(args.pageId);
    if (!page) throw new Error("Page not found");

    await ctx.db.delete(args.pageId);

    // Update BEP's updatedAt
    await ctx.db.patch(page.bepId, { updatedAt: Date.now() });
  },
});

export const reorderPages = mutation({
  args: {
    bepId: v.id("beps"),
    pageIds: v.array(v.id("bepPages")),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    // Update order for each page based on position in array
    await Promise.all(
      args.pageIds.map((pageId, index) =>
        ctx.db.patch(pageId, { order: index, updatedAt: now })
      )
    );

    // Update BEP's updatedAt
    await ctx.db.patch(args.bepId, { updatedAt: now });
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// IMPORT MUTATION (imports content and optionally creates a new version)
// ─────────────────────────────────────────────────────────────────────────────

export const importVersion = mutation({
  args: {
    bepId: v.id("beps"),
    content: v.string(), // Clean main content (README)
    pages: v.array(
      v.object({
        slug: v.string(),
        title: v.string(),
        content: v.string(),
      })
    ),
    editNote: v.optional(v.string()),
    userId: v.id("users"),
    versionMode: v.optional(v.union(v.literal("new"), v.literal("current"))),
  },
  handler: async (ctx, args) => {
    // 1. Get current BEP
    const bep = await ctx.db.get(args.bepId);
    if (!bep) throw new Error("BEP not found");

    // 2. Get latest version number
    const latestVersion = await ctx.db
      .query("bepVersions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .order("desc")
      .first();

    const shouldCreateNewVersion =
      args.versionMode === undefined ||
      args.versionMode === "new" ||
      !latestVersion;
    const newVersionNumber = (latestVersion?.version ?? 0) + 1;
    const now = Date.now();

    // 3. Update BEP's main content
    await ctx.db.patch(args.bepId, {
      content: args.content,
      updatedAt: now,
    });

    // 4. Handle pages - update existing or create new
    const existingPages = await ctx.db
      .query("bepPages")
      .withIndex("by_bep_order", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Create a map of existing pages by slug
    const existingPagesBySlug = new Map(
      existingPages.map((p) => [p.slug, p])
    );

    // Find the highest order number for new pages
    let maxOrder = existingPages.reduce((max, p) => Math.max(max, p.order), -1);

    // Track which pages were processed (for the version snapshot)
    const processedPages: Array<{
      slug: string;
      title: string;
      content: string;
      order: number;
    }> = [];

    for (const importedPage of args.pages) {
      const existingPage = existingPagesBySlug.get(importedPage.slug);

      if (existingPage) {
        // Update existing page
        await ctx.db.patch(existingPage._id, {
          title: importedPage.title,
          content: importedPage.content,
          updatedAt: now,
        });
        processedPages.push({
          slug: importedPage.slug,
          title: importedPage.title,
          content: importedPage.content,
          order: existingPage.order,
        });
      } else {
        // Create new page
        maxOrder += 1;
        await ctx.db.insert("bepPages", {
          bepId: args.bepId,
          slug: importedPage.slug,
          title: importedPage.title,
          content: importedPage.content,
          order: maxOrder,
          createdAt: now,
          updatedAt: now,
        });
        processedPages.push({
          slug: importedPage.slug,
          title: importedPage.title,
          content: importedPage.content,
          order: maxOrder,
        });
      }
    }

    // Include existing pages that weren't in the import
    for (const existingPage of existingPages) {
      if (!args.pages.some((p) => p.slug === existingPage.slug)) {
        processedPages.push({
          slug: existingPage.slug,
          title: existingPage.title,
          content: existingPage.content,
          order: existingPage.order,
        });
      }
    }

    // Sort by order for the snapshot
    processedPages.sort((a, b) => a.order - b.order);

    const pagesCreated = args.pages.filter(
      (p) => !existingPagesBySlug.has(p.slug)
    ).length;
    const pagesUpdated = args.pages.filter((p) =>
      existingPagesBySlug.has(p.slug)
    ).length;

    if (shouldCreateNewVersion) {
      // 5a. Create a new version snapshot
      const versionId = await ctx.db.insert("bepVersions", {
        bepId: args.bepId,
        version: newVersionNumber,
        title: bep.title, // Keep the existing title
        content: args.content,
        pagesSnapshot: processedPages.length > 0 ? processedPages : undefined,
        editedBy: args.userId,
        editNote: args.editNote ?? "Imported from markdown",
        createdAt: now,
      });

      // 6a. Trigger version analysis if this is v2 or later
      if (newVersionNumber >= 2 && latestVersion) {
        await ctx.scheduler.runAfter(0, internal.analysisJobs.create, {
          bepId: args.bepId,
          versionId,
          previousVersionId: latestVersion._id,
        });
      }

      return {
        versionId,
        versionNumber: newVersionNumber,
        versionAction: "created" as const,
        pagesCreated,
        pagesUpdated,
      };
    }

    // 5b. Update the current version in place
    if (!latestVersion) {
      throw new Error("Latest version not found");
    }

    const versionId = latestVersion._id;
    await ctx.db.patch(latestVersion._id, {
      title: bep.title,
      content: args.content,
      pagesSnapshot: processedPages.length > 0 ? processedPages : undefined,
      editedBy: args.userId,
      editNote: args.editNote ?? latestVersion.editNote,
    });

    return {
      versionId,
      versionNumber: latestVersion.version,
      versionAction: "updated" as const,
      pagesCreated,
      pagesUpdated,
    };
  },
});
