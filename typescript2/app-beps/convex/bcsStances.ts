// Where team members stand on a BCS proposal, per version, as BEP stances: support, neutral or
// oppose, with an optional reason. Like bcs.ts, every function takes the server secret.
import { v } from "convex/values";
import { mutation, query } from "./_generated/server";
import type { QueryCtx } from "./_generated/server";
import { byNumber, fail, versionRow } from "./bcs";
import { userStance } from "./schema";
import { requireServerSecret } from "./serverSecret";

/** The longest reason, which keeps the stance list readable. */
export const MAX_STANCE_CHARS = 500;

async function versionOrFail(ctx: QueryCtx, number: number, version: number) {
  const doc = await byNumber(ctx, number);
  if (!doc) fail("missing", "No such proposal.");
  if (!(await versionRow(ctx, doc._id, version))) fail("missing", "No such version.");
  return doc;
}

/** The stances on one version, newest first, with how many of each. */
export const list = query({
  args: { secret: v.string(), number: v.number(), version: v.number() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) return null;
    const stances = await ctx.db
      .query("bcsStances")
      .withIndex("by_doc_version", (q) => q.eq("docId", doc._id).eq("version", args.version))
      .collect();
    const summary = { support: 0, neutral: 0, oppose: 0 };
    for (const each of stances) summary[each.stance]++;
    return {
      summary,
      stances: stances
        .sort((a, b) => b.updatedAt - a.updatedAt)
        .map((each) => ({
          author: each.author,
          stance: each.stance,
          comment: each.comment,
          updatedAt: each.updatedAt,
        })),
    };
  },
});

/** Takes or changes the member's stance on a version. */
export const set = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    version: v.number(),
    stance: userStance,
    comment: v.optional(v.string()),
    author: v.string(),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await versionOrFail(ctx, args.number, args.version);
    const comment = args.comment?.trim() || undefined;
    if (comment && comment.length > MAX_STANCE_CHARS) {
      fail("invalid", `The reason is longer than ${MAX_STANCE_CHARS} characters.`);
    }
    const now = Date.now();
    const existing = await ctx.db
      .query("bcsStances")
      .withIndex("by_doc_version_author", (q) =>
        q.eq("docId", doc._id).eq("version", args.version).eq("author", args.author)
      )
      .unique();
    if (existing) {
      await ctx.db.patch(existing._id, { stance: args.stance, comment, updatedAt: now });
    } else {
      await ctx.db.insert("bcsStances", {
        docId: doc._id,
        version: args.version,
        author: args.author,
        stance: args.stance,
        comment,
        createdAt: now,
        updatedAt: now,
      });
    }
  },
});

/** Withdraws the member's stance on a version. */
export const clear = mutation({
  args: { secret: v.string(), number: v.number(), version: v.number(), author: v.string() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await versionOrFail(ctx, args.number, args.version);
    const existing = await ctx.db
      .query("bcsStances")
      .withIndex("by_doc_version_author", (q) =>
        q.eq("docId", doc._id).eq("version", args.version).eq("author", args.author)
      )
      .unique();
    if (existing) await ctx.db.delete(existing._id);
  },
});
