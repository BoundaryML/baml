// BCS design proposals: team-only system design docs, each a whole HTML page with versions.
//
// Every function takes `secret`, which must equal the deployment's BCS_CONVEX_SECRET. Only this
// app's server holds it, and it calls these functions after it has checked that the viewer is on
// the team (src/lib/bcs-auth.ts), so the browser can neither read nor write these tables.
import { v } from "convex/values";
import { mutation, query } from "./_generated/server";
import type { QueryCtx } from "./_generated/server";
import type { Id } from "./_generated/dataModel";
import { bepStatus } from "./schema";

/** The longest HTML page a version may hold, under Convex's 1 MiB document limit. */
export const MAX_HTML_BYTES = 900_000;

/** Refuses a call without the server secret, comparing in constant time. */
function guard(secret: string) {
  const expected = process.env.BCS_CONVEX_SECRET;
  if (!expected || expected.length < 32) {
    throw new Error("BCS proposals are not configured");
  }
  let difference = secret.length === expected.length ? 0 : 1;
  for (let i = 0; i < expected.length; i++) {
    difference |= expected.charCodeAt(i) ^ (i < secret.length ? secret.charCodeAt(i) : 0);
  }
  if (difference !== 0) {
    throw new Error("Not allowed");
  }
}

function checkHtml(html: string) {
  if (!html.trim()) {
    throw new Error("The page is empty");
  }
  if (new TextEncoder().encode(html).length > MAX_HTML_BYTES) {
    throw new Error("The page is larger than 900 KB");
  }
}

async function byNumber(ctx: QueryCtx, number: number) {
  return ctx.db
    .query("bcsDocs")
    .withIndex("by_number", (q) => q.eq("number", number))
    .unique();
}

export const list = query({
  args: { secret: v.string() },
  handler: async (ctx, args) => {
    guard(args.secret);
    const docs = await ctx.db.query("bcsDocs").withIndex("by_number").order("desc").collect();
    return docs.map((doc) => ({
      number: doc.number,
      title: doc.title,
      status: doc.status,
      authors: doc.authors,
      currentVersion: doc.currentVersion,
      updatedAt: doc.updatedAt,
    }));
  },
});

/** A version's page: one document, read by itself. */
async function page(ctx: QueryCtx, docId: Id<"bcsDocs">, version: number) {
  return ctx.db
    .query("bcsVersionPages")
    .withIndex("by_doc_version", (q) => q.eq("docId", docId).eq("version", version))
    .unique();
}

/** A proposal with one version's page, its current one unless `version` names another. */
export const get = query({
  args: { secret: v.string(), number: v.number(), version: v.optional(v.number()) },
  handler: async (ctx, args) => {
    guard(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) return null;
    const version = args.version ?? doc.currentVersion;
    const shown = await page(ctx, doc._id, version);
    if (!shown) return null;
    const versions = await ctx.db
      .query("bcsDocVersions")
      .withIndex("by_doc_version", (q) => q.eq("docId", doc._id))
      .order("desc")
      .collect();
    return {
      number: doc.number,
      title: doc.title,
      status: doc.status,
      authors: doc.authors,
      currentVersion: doc.currentVersion,
      createdAt: doc.createdAt,
      updatedAt: doc.updatedAt,
      version,
      html: shown.html,
      versions: versions.map((each) => ({
        version: each.version,
        note: each.note,
        createdBy: each.createdBy,
        createdAt: each.createdAt,
      })),
    };
  },
});

export const create = mutation({
  args: {
    secret: v.string(),
    title: v.string(),
    status: bepStatus,
    html: v.string(),
    author: v.string(),
  },
  handler: async (ctx, args) => {
    guard(args.secret);
    const title = args.title.trim();
    if (!title) throw new Error("A proposal needs a title");
    checkHtml(args.html);
    const latest = await ctx.db.query("bcsDocs").withIndex("by_number").order("desc").first();
    const number = (latest?.number ?? 0) + 1;
    const now = Date.now();
    const docId = await ctx.db.insert("bcsDocs", {
      number,
      title,
      status: args.status,
      authors: [args.author],
      currentVersion: 1,
      createdAt: now,
      updatedAt: now,
    });
    await ctx.db.insert("bcsDocVersions", { docId, version: 1, createdBy: args.author, createdAt: now });
    await ctx.db.insert("bcsVersionPages", { docId, version: 1, html: args.html });
    return number;
  },
});

/**
 * Saves what an editor changed, in one transaction: a new version when `html` differs from the
 * current page, and the title or status when given. Refused when `baseVersion`, the version the
 * editor started from, is no longer the current one, so a stale editor cannot hide a newer save.
 */
export const save = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    author: v.string(),
    baseVersion: v.optional(v.number()),
    html: v.optional(v.string()),
    note: v.optional(v.string()),
    title: v.optional(v.string()),
    status: v.optional(bepStatus),
  },
  handler: async (ctx, args) => {
    guard(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) throw new Error("No such proposal");
    if (args.baseVersion !== undefined && args.baseVersion !== doc.currentVersion) {
      throw new Error(
        `Someone saved v${doc.currentVersion} since you started from v${args.baseVersion}: reload it and save again`
      );
    }
    const now = Date.now();
    let version = doc.currentVersion;
    if (args.html !== undefined) {
      checkHtml(args.html);
      const current = await page(ctx, doc._id, doc.currentVersion);
      if (current?.html !== args.html) {
        version = doc.currentVersion + 1;
        await ctx.db.insert("bcsDocVersions", {
          docId: doc._id,
          version,
          note: args.note?.trim() || undefined,
          createdBy: args.author,
          createdAt: now,
        });
        await ctx.db.insert("bcsVersionPages", { docId: doc._id, version, html: args.html });
      }
    }
    const title = args.title?.trim();
    const changed = version !== doc.currentVersion || title || args.status;
    if (changed) {
      await ctx.db.patch(doc._id, {
        currentVersion: version,
        updatedAt: now,
        ...(title ? { title } : {}),
        ...(args.status ? { status: args.status } : {}),
        authors: doc.authors.includes(args.author) ? doc.authors : [...doc.authors, args.author],
      });
    }
    return version;
  },
});
