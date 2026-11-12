// BCS design proposals: team-only system design docs, each a whole HTML page with versions.
//
// Every function takes `secret`, which must equal the deployment's BCS_CONVEX_SECRET. Only this
// app's server holds it, and it calls these functions after it has checked that the viewer is on
// the team (src/lib/bcs-auth.ts), so the browser can neither read nor write these tables.
//
// Errors meant for people are ConvexErrors carrying a `BcsFailure`: production Convex replaces the
// message of any other error with "Server Error", so a stale save would otherwise reach the editor
// as a crash.
import { ConvexError, v } from "convex/values";
import { internalMutation, internalQuery, mutation, query } from "./_generated/server";
import type { MutationCtx, QueryCtx } from "./_generated/server";
import type { Id } from "./_generated/dataModel";
import { internal } from "./_generated/api";
import { bepStatus } from "./schema";
import { requireServerSecret } from "./serverSecret";

/**
 * Why a BCS call was refused. `not_configured`: the deployments' BCS_CONVEX_SECRET is missing or
 * differs from the server's. `stale`: someone saved since the editor started. `missing`: no such
 * proposal. `invalid`: the request itself is wrong (empty page, no title, too large).
 */
export type BcsFailure = {
  kind: "not_configured" | "stale" | "missing" | "invalid";
  message: string;
  // For `stale`: the version someone else saved, which the editor can compare and save over.
  currentVersion?: number;
};

function fail(kind: BcsFailure["kind"], message: string, currentVersion?: number): never {
  throw new ConvexError<BcsFailure>({ kind, message, currentVersion });
}

/** The longest HTML page a version may hold, under Convex's 1 MiB document limit. */
export const MAX_HTML_BYTES = 900_000;

/** Refuses a call without the server secret. */
function guard(secret: string) {
  requireServerSecret(secret);
}

function checkHtml(html: string) {
  if (!html.trim()) {
    fail("invalid", "The page is empty.");
  }
  if (new TextEncoder().encode(html).length > MAX_HTML_BYTES) {
    fail("invalid", "The page is larger than 900 KB.");
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
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    guard(args.secret);
    const title = args.title.trim();
    if (!title) fail("invalid", "A proposal needs a title.");
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
    await tellSlack(ctx, docId, { kind: "created" }, args.author, args.authorGithubId);
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
    authorGithubId: v.optional(v.string()),
    baseVersion: v.optional(v.number()),
    html: v.optional(v.string()),
    note: v.optional(v.string()),
    title: v.optional(v.string()),
    status: v.optional(bepStatus),
  },
  handler: async (ctx, args) => {
    guard(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) fail("missing", "No such proposal.");
    if (args.baseVersion !== undefined && args.baseVersion !== doc.currentVersion) {
      fail(
        "stale",
        `Someone saved v${doc.currentVersion} since you started from v${args.baseVersion}: reload it and save again.`,
        doc.currentVersion
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
    const newVersion = version !== doc.currentVersion;
    const title = args.title?.trim() || undefined;
    const newTitle = title !== undefined && title !== doc.title;
    const newStatus = args.status !== undefined && args.status !== doc.status;
    if (newVersion || newTitle || newStatus) {
      await ctx.db.patch(doc._id, {
        currentVersion: version,
        updatedAt: now,
        ...(newTitle ? { title } : {}),
        ...(newStatus ? { status: args.status } : {}),
        authors: doc.authors.includes(args.author) ? doc.authors : [...doc.authors, args.author],
      });
      await tellSlack(
        ctx,
        doc._id,
        {
          kind: "saved",
          version: newVersion ? version : undefined,
          status: newStatus ? { from: doc.status, to: args.status! } : undefined,
        },
        args.author,
        args.authorGithubId
      );
    }
    return version;
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// Slack: each proposal gets a thread in the BCS channel (bcsSlack.ts). These are internal, so only
// this deployment's own functions can call them.
// ─────────────────────────────────────────────────────────────────────────────

export const slackEvent = v.union(
  v.object({ kind: v.literal("created") }),
  v.object({
    kind: v.literal("saved"),
    version: v.optional(v.number()),
    status: v.optional(v.object({ from: bepStatus, to: bepStatus })),
  })
);

async function tellSlack(
  ctx: MutationCtx,
  docId: Id<"bcsDocs">,
  event: typeof slackEvent.type,
  login: string,
  githubId: string | undefined
) {
  await ctx.scheduler.runAfter(0, internal.bcsSlack.notify, { docId, event, login, githubId });
}

/** What a proposal's Slack messages show, and its thread. */
export const slackView = internalQuery({
  args: { docId: v.id("bcsDocs"), version: v.optional(v.number()), githubId: v.optional(v.string()) },
  handler: async (ctx, args) => {
    const doc = await ctx.db.get(args.docId);
    if (!doc) return null;
    const version =
      args.version === undefined
        ? null
        : await ctx.db
            .query("bcsDocVersions")
            .withIndex("by_doc_version", (q) => q.eq("docId", doc._id).eq("version", args.version!))
            .unique();
    // The member's Slack account, linked when they signed in with a verified BoundaryML email.
    const user = args.githubId
      ? await ctx.db
          .query("users")
          .withIndex("by_github_id", (q) => q.eq("githubId", args.githubId))
          .unique()
      : null;
    return {
      number: doc.number,
      title: doc.title,
      status: doc.status,
      authors: doc.authors,
      currentVersion: doc.currentVersion,
      updatedAt: doc.updatedAt,
      slackThreadTs: doc.slackThreadTs,
      note: version?.note,
      slackUserId: user?.slackUserId,
    };
  },
});

/**
 * Keeps `ts` as the proposal's thread unless another message got there first, and returns the
 * thread that won, so a notification that lost the race can delete its message and reply there.
 */
export const storeSlackThread = internalMutation({
  args: { docId: v.id("bcsDocs"), ts: v.string() },
  handler: async (ctx, args) => {
    const doc = await ctx.db.get(args.docId);
    if (!doc) return null;
    if (doc.slackThreadTs) return doc.slackThreadTs;
    await ctx.db.patch(args.docId, { slackThreadTs: args.ts });
    return args.ts;
  },
});
