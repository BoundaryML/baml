// BCS design proposals: team-only system design docs with versions. Each version has a main page
// and optional sub-pages (a tree, like BEP pages), each Markdown with raw HTML allowed or a whole
// HTML page; the site renders them in a sandboxed frame.
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
import { bcsPageFormat, bcsPageOutline, bepStatus } from "./schema";
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

export function fail(kind: BcsFailure["kind"], message: string, currentVersion?: number): never {
  throw new ConvexError<BcsFailure>({ kind, message, currentVersion });
}

/** The longest page a version may hold, under Convex's 1 MiB document limit. */
export const MAX_PAGE_BYTES = 900_000;
/** The most pages a proposal may have, main page included. */
export const MAX_PAGES = 50;
/**
 * The most a version's pages may weigh together, as the JSON `bep bcs` sends and receives in one
 * request: Vercel refuses bodies over 4.5 MB, and escaping can make JSON several times its text.
 */
export const MAX_VERSION_JSON_BYTES = 4_000_000;

const SLUG = /^[a-z0-9][a-z0-9-]{0,63}$/;

/** Refuses a call without the server secret. */
function guard(secret: string) {
  requireServerSecret(secret);
}

type PageFormat = typeof bcsPageFormat.type;
type Outline = typeof bcsPageOutline.type;
/** A page as an editor sends it: its title comes from its content. */
type PageInput = { slug: string; parentSlug?: string; format: PageFormat; content: string };
type Page = Outline & { content: string };

export const pageInput = v.object({
  slug: v.string(),
  parentSlug: v.optional(v.string()),
  format: bcsPageFormat,
  content: v.string(),
});

function bytes(text: string): number {
  return new TextEncoder().encode(text).length;
}

/**
 * A sub-page's title: its first Markdown `# ` heading, or an HTML page's <title> or first <h1>,
 * else its slug. The main page's title is the proposal's.
 */
export function pageTitle(page: PageInput): string {
  const fromMarkdown = page.content.match(/^#[ \t]+(.+?)[ \t#]*$/m)?.[1];
  const fromHtml =
    page.content.match(/<title[^>]*>([^<]+)<\/title>/i)?.[1] ??
    page.content.match(/<h1[^>]*>([\s\S]*?)<\/h1>/i)?.[1]?.replace(/<[^>]+>/g, "");
  const found = (page.format === "markdown" ? fromMarkdown ?? fromHtml : fromHtml ?? fromMarkdown)?.trim();
  return found ? found.slice(0, 200) : page.slug;
}

/**
 * Checks a version's pages and returns them with titles: one main page (slug ""), sub-page slugs
 * unique and URL-safe, each parent another sub-page, no cycles, nothing empty or too large.
 */
function checkPages(inputs: PageInput[], title: string): Page[] {
  if (inputs.length > MAX_PAGES) fail("invalid", `A proposal can have at most ${MAX_PAGES} pages.`);
  const mains = inputs.filter((page) => page.slug === "");
  if (mains.length !== 1) fail("invalid", "A proposal needs exactly one main page.");
  const slugs = new Set<string>();
  for (const page of inputs) {
    const name = page.slug || "the main page";
    if (page.slug !== "" && !SLUG.test(page.slug)) {
      fail("invalid", `"${page.slug}" is not a page slug: use lowercase letters, digits and dashes.`);
    }
    if (slugs.has(page.slug)) fail("invalid", `Two pages are called "${page.slug}".`);
    slugs.add(page.slug);
    if (!page.content.trim()) fail("invalid", `${name === "the main page" ? "The main page" : `Page "${name}"`} is empty.`);
    const size = bytes(page.content);
    if (size > MAX_PAGE_BYTES) fail("invalid", `Page "${name}" is larger than 900 KB.`);
  }
  if (bytes(JSON.stringify(inputs)) > MAX_VERSION_JSON_BYTES) {
    fail("invalid", "The pages together are too large: over 4 MB as JSON, which bep bcs could not send.");
  }
  const parents = new Map(inputs.map((page) => [page.slug, page.parentSlug]));
  for (const page of inputs) {
    if (page.parentSlug === undefined) continue;
    if (page.slug === "") fail("invalid", "The main page cannot have a parent.");
    if (page.parentSlug === "" || !slugs.has(page.parentSlug)) {
      fail("invalid", `Page "${page.slug}" names a parent page "${page.parentSlug}" that does not exist.`);
    }
    // Walk up: a parent chain that comes back to this page is a cycle.
    const seen = new Set([page.slug]);
    for (let up: string | undefined = page.parentSlug; up !== undefined; up = parents.get(up)) {
      if (seen.has(up)) fail("invalid", `Page "${page.slug}" ends up inside itself: its parents loop back to it.`);
      seen.add(up);
    }
  }
  return inputs.map((page) => ({
    slug: page.slug,
    title: page.slug === "" ? title : pageTitle(page),
    parentSlug: page.parentSlug,
    format: page.format,
    content: page.content,
  }));
}

export async function byNumber(ctx: QueryCtx, number: number) {
  return ctx.db
    .query("bcsDocs")
    .withIndex("by_number", (q) => q.eq("number", number))
    .unique();
}

/**
 * Every proposal, with what the board shows (shepherds, open comments, page count), newest first;
 * and every GitHub login seen across proposals, to suggest when choosing shepherds.
 */
export const list = query({
  args: { secret: v.string() },
  handler: async (ctx, args) => {
    guard(args.secret);
    const docs = await ctx.db.query("bcsDocs").withIndex("by_number").order("desc").collect();
    const people = new Set<string>();
    const proposals = [];
    for (const doc of docs) {
      // The comment mutations keep these on the proposal; only one from before they did is counted
      // from its comments (until its next comment, which stores them).
      let openComments = doc.openComments;
      let commenters = doc.commenters;
      if (openComments === undefined || commenters === undefined) {
        const comments = await ctx.db
          .query("bcsComments")
          .withIndex("by_doc", (q) => q.eq("docId", doc._id))
          .collect();
        openComments = comments.filter((each) => !each.parentId && !each.resolved).length;
        commenters = comments.map((each) => each.author);
      }
      const row = await versionRow(ctx, doc._id, doc.currentVersion);
      for (const login of [
        ...doc.authors,
        ...(doc.shepherds ?? []),
        ...(doc.implementedBy ?? []),
        ...commenters,
      ]) {
        people.add(login);
      }
      proposals.push({
        number: doc.number,
        title: doc.title,
        status: doc.status,
        authors: doc.authors,
        shepherds: doc.shepherds ?? [],
        implementedBy: doc.implementedBy ?? [],
        currentVersion: doc.currentVersion,
        updatedAt: doc.updatedAt,
        openComments,
        pageCount: row ? outlineOf(row, doc.title).length : 1,
      });
    }
    return { proposals, people: [...people].sort((a, b) => a.localeCompare(b)) };
  },
});

const LOGIN = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$/;

/** GitHub logins from an editor: trimmed, without a leading @, unique, each a valid login. */
function checkLogins(logins: string[], what: string): string[] {
  const result: string[] = [];
  for (const raw of logins) {
    const login = raw.trim().replace(/^@/, "");
    if (!login) continue;
    if (!LOGIN.test(login)) fail("invalid", `"${raw.trim()}" is not a GitHub login (${what}).`);
    if (!result.some((each) => each.toLowerCase() === login.toLowerCase())) result.push(login);
  }
  if (result.length > 20) fail("invalid", `At most 20 ${what}.`);
  return result;
}

export async function versionRow(ctx: QueryCtx, docId: Id<"bcsDocs">, version: number) {
  return ctx.db
    .query("bcsDocVersions")
    .withIndex("by_doc_version", (q) => q.eq("docId", docId).eq("version", version))
    .unique();
}

/** A version's outline; one saved before sub-pages existed is its HTML main page. */
export function outlineOf(row: { pages?: Outline[] }, title: string): Outline[] {
  return row.pages ?? [{ slug: "", title, format: "html" }];
}

/** One page's content and format, read by itself. */
async function pageContent(ctx: QueryCtx, docId: Id<"bcsDocs">, version: number, slug: string) {
  const row = await ctx.db
    .query("bcsVersionPages")
    .withIndex("by_doc_version_slug", (q) => q.eq("docId", docId).eq("version", version).eq("slug", slug))
    .unique();
  if (row) return { format: row.format ?? "html", content: row.content ?? row.html ?? "" };
  if (slug !== "") return null;
  // A version saved before sub-pages existed: its one row, without a slug, is the main page.
  const legacy = await ctx.db
    .query("bcsVersionPages")
    .withIndex("by_doc_version", (q) => q.eq("docId", docId).eq("version", version))
    .first();
  return legacy && legacy.slug === undefined
    ? { format: "html" as const, content: legacy.html ?? legacy.content ?? "" }
    : null;
}

/** Every page of a version with its content, in outline order. */
async function allPages(ctx: QueryCtx, docId: Id<"bcsDocs">, version: number, title: string) {
  const row = await versionRow(ctx, docId, version);
  if (!row) return null;
  const pages: Page[] = [];
  for (const outline of outlineOf(row, title)) {
    const found = await pageContent(ctx, docId, version, outline.slug);
    if (found) pages.push({ ...outline, format: found.format, content: found.content });
  }
  return pages;
}

/**
 * A proposal with one page of one version: its current version and main page unless `version`
 * or `page` (a slug) names another. The outline lists every page of that version.
 */
export const get = query({
  args: {
    secret: v.string(),
    number: v.number(),
    version: v.optional(v.number()),
    page: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    guard(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) return null;
    const version = args.version ?? doc.currentVersion;
    const row = await versionRow(ctx, doc._id, version);
    if (!row) return null;
    const outline = outlineOf(row, doc.title).map((page) =>
      page.slug === "" ? { ...page, title: doc.title } : page
    );
    const slug = args.page ?? "";
    const shown = outline.find((page) => page.slug === slug);
    const found = shown && (await pageContent(ctx, doc._id, version, slug));
    if (!shown || !found) return null;
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
      shepherds: doc.shepherds ?? [],
      implementedBy: doc.implementedBy ?? [],
      currentVersion: doc.currentVersion,
      createdAt: doc.createdAt,
      updatedAt: doc.updatedAt,
      version,
      outline,
      page: { ...shown, format: found.format, content: found.content },
      versions: versions.map((each) => ({
        version: each.version,
        note: each.note,
        createdBy: each.createdBy,
        createdAt: each.createdAt,
        changed: each.changed,
      })),
    };
  },
});

/** Every page of a proposal's version with its content, for the `bep bcs` CLI. */
export const getAll = query({
  args: { secret: v.string(), number: v.number(), version: v.optional(v.number()) },
  handler: async (ctx, args) => {
    guard(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) return null;
    const version = args.version ?? doc.currentVersion;
    const pages = await allPages(ctx, doc._id, version, doc.title);
    if (!pages) return null;
    return {
      number: doc.number,
      title: doc.title,
      status: doc.status,
      authors: doc.authors,
      version,
      updatedAt: doc.updatedAt,
      pages: pages.map((page) => (page.slug === "" ? { ...page, title: doc.title } : page)),
    };
  },
});

/** Stores a new version's outline and pages. */
async function storeVersion(
  ctx: MutationCtx,
  docId: Id<"bcsDocs">,
  version: number,
  pages: Page[],
  meta: { note?: string; createdBy: string; createdAt: number; changed?: string[] }
) {
  await ctx.db.insert("bcsDocVersions", {
    docId,
    version,
    note: meta.note,
    createdBy: meta.createdBy,
    createdAt: meta.createdAt,
    changed: meta.changed,
    pages: pages.map(({ slug, title, parentSlug, format }) => ({ slug, title, parentSlug, format })),
  });
  for (const page of pages) {
    await ctx.db.insert("bcsVersionPages", {
      docId,
      version,
      slug: page.slug,
      format: page.format,
      content: page.content,
    });
  }
}

export const create = mutation({
  args: {
    secret: v.string(),
    title: v.string(),
    status: bepStatus,
    // The main page and any sub-pages; `html` is the main page alone, from older `bep` scripts.
    pages: v.optional(v.array(pageInput)),
    html: v.optional(v.string()),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    guard(args.secret);
    const title = args.title.trim();
    if (!title) fail("invalid", "A proposal needs a title.");
    const inputs =
      args.pages ??
      (args.html !== undefined ? [{ slug: "", format: "html" as const, content: args.html }] : []);
    const pages = checkPages(inputs, title);
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
    await storeVersion(ctx, docId, 1, pages, { createdBy: args.author, createdAt: now });
    await tellSlack(ctx, docId, { kind: "created" }, args.author, args.authorGithubId);
    return number;
  },
});

/** The pages a save leaves, from the current ones and what the editor sent. */
function applyChanges(
  current: Page[],
  change: {
    pages?: PageInput[];
    html?: string;
    upsert?: PageInput;
    add?: PageInput;
    remove?: string;
  }
): PageInput[] {
  if (change.pages) return change.pages;
  let pages: PageInput[] = current.map(({ slug, parentSlug, format, content }) => ({
    slug,
    parentSlug,
    format,
    content,
  }));
  // An older `bep` script sends the main page alone as HTML.
  const upsert = change.upsert ?? (change.html !== undefined ? { slug: "", format: "html" as const, content: change.html } : undefined);
  if (upsert) {
    const at = pages.findIndex((page) => page.slug === upsert.slug);
    if (at >= 0) {
      // The editor sends where the page sits: no parent means the top level.
      pages[at] = { ...upsert, parentSlug: upsert.slug === "" ? undefined : upsert.parentSlug };
    } else {
      pages.push(upsert);
    }
  }
  if (change.add) {
    const added = change.add;
    if (pages.some((page) => page.slug === added.slug)) {
      fail("invalid", `There is already a page called "${added.slug}".`);
    }
    pages.push(added);
  }
  if (change.remove !== undefined) {
    if (change.remove === "") fail("invalid", "The main page cannot be removed.");
    if (!pages.some((page) => page.slug === change.remove)) fail("missing", "No such page.");
    if (pages.some((page) => page.parentSlug === change.remove)) {
      fail("invalid", "Move or remove this page's sub-pages first.");
    }
    pages = pages.filter((page) => page.slug !== change.remove);
  }
  return pages;
}

/** Titles of pages that differ between two versions, including added and removed ones. */
function changedTitles(before: Page[], after: Page[]): string[] {
  const old = new Map(before.map((page) => [page.slug, page]));
  const now = new Map(after.map((page) => [page.slug, page]));
  const titles: string[] = [];
  for (const page of after) {
    const was = old.get(page.slug);
    if (
      !was ||
      was.content !== page.content ||
      was.format !== page.format ||
      was.parentSlug !== page.parentSlug ||
      was.title !== page.title
    ) {
      titles.push(page.title);
    }
  }
  for (const page of before) if (!now.has(page.slug)) titles.push(page.title);
  const sameOrder = before.map((page) => page.slug).join("/") === after.map((page) => page.slug).join("/");
  return titles.length === 0 && !sameOrder ? ["page order"] : titles;
}

/**
 * Saves what an editor changed, in one transaction: a new version when the pages differ from the
 * current ones, and the title or status when given. Pages come as the whole set (`pages`), one
 * page replaced or added with its parent (`upsert`, no parent being the top level), one added that must be new (`add`), one removed (`remove`),
 * or, from older `bep` scripts, the
 * main page as `html`. Refused when `baseVersion`, the version the editor started from, is no
 * longer the current one, so a stale editor cannot hide a newer save.
 */
export const save = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
    baseVersion: v.optional(v.number()),
    pages: v.optional(v.array(pageInput)),
    upsert: v.optional(pageInput),
    add: v.optional(pageInput),
    remove: v.optional(v.string()),
    html: v.optional(v.string()),
    note: v.optional(v.string()),
    title: v.optional(v.string()),
    status: v.optional(bepStatus),
    shepherds: v.optional(v.array(v.string())),
    implementedBy: v.optional(v.array(v.string())),
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
    const title = args.title?.trim() || undefined;
    const newTitle = title !== undefined && title !== doc.title;
    const newStatus = args.status !== undefined && args.status !== doc.status;
    const shepherds = args.shepherds && checkLogins(args.shepherds, "shepherds");
    const implementedBy = args.implementedBy && checkLogins(args.implementedBy, "implementers");
    const same = (a: string[] | undefined, b: string[]) => (a ?? []).join("\n") === b.join("\n");
    const newShepherds = shepherds !== undefined && !same(doc.shepherds, shepherds);
    const newImplementers = implementedBy !== undefined && !same(doc.implementedBy, implementedBy);
    let version = doc.currentVersion;
    const touchesPages =
      args.pages !== undefined ||
      args.upsert !== undefined ||
      args.add !== undefined ||
      args.remove !== undefined ||
      args.html !== undefined;
    if (touchesPages) {
      const current = (await allPages(ctx, doc._id, doc.currentVersion, doc.title)) ?? [];
      const pages = checkPages(applyChanges(current, args), title ?? doc.title);
      const changed = changedTitles(current, pages);
      if (changed.length > 0) {
        version = doc.currentVersion + 1;
        await storeVersion(ctx, doc._id, version, pages, {
          note: args.note?.trim() || undefined,
          createdBy: args.author,
          createdAt: now,
          changed,
        });
      }
    }
    const newVersion = version !== doc.currentVersion;
    if (newVersion || newTitle || newStatus || newShepherds || newImplementers) {
      await ctx.db.patch(doc._id, {
        currentVersion: version,
        updatedAt: now,
        ...(newTitle ? { title } : {}),
        ...(newStatus ? { status: args.status } : {}),
        ...(newShepherds ? { shepherds } : {}),
        ...(newImplementers ? { implementedBy } : {}),
        // Writing a version or renaming makes the editor an author; status and shepherds do not.
        ...((newVersion || newTitle) && !doc.authors.includes(args.author)
          ? { authors: [...doc.authors, args.author] }
          : {}),
      });
      await tellSlack(
        ctx,
        doc._id,
        {
          kind: "saved",
          version: newVersion ? version : undefined,
          status: newStatus ? { from: doc.status, to: args.status! } : undefined,
          shepherds: newShepherds ? shepherds : undefined,
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
    shepherds: v.optional(v.array(v.string())),
  }),
  v.object({ kind: v.literal("comment"), commentId: v.id("bcsComments") }),
  v.object({ kind: v.literal("resolved"), commentId: v.id("bcsComments"), resolved: v.boolean() })
);

export async function tellSlack(
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
  args: {
    docId: v.id("bcsDocs"),
    version: v.optional(v.number()),
    githubId: v.optional(v.string()),
    commentId: v.optional(v.id("bcsComments")),
  },
  handler: async (ctx, args) => {
    const doc = await ctx.db.get(args.docId);
    if (!doc) return null;
    const comments = await ctx.db
      .query("bcsComments")
      .withIndex("by_doc", (q) => q.eq("docId", doc._id))
      .collect();
    // Shown once a proposal has threads, then kept, so resolving the last one shows 0.
    const threads = comments.filter((each) => !each.parentId);
    const openComments = threads.length > 0 ? threads.filter((each) => !each.resolved).length : undefined;
    const comment = args.commentId ? comments.find((each) => each._id === args.commentId) : undefined;
    const thread = comment?.parentId ? comments.find((each) => each._id === comment.parentId) : comment;
    const outline = outlineOf((await versionRow(ctx, doc._id, doc.currentVersion)) ?? {}, doc.title);
    const pageTitle = (slug: string) =>
      slug === "" ? doc.title : outline.find((page) => page.slug === slug)?.title ?? slug;
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
      shepherds: doc.shepherds ?? [],
      currentVersion: doc.currentVersion,
      updatedAt: doc.updatedAt,
      slackThreadTs: doc.slackThreadTs,
      note: version?.note,
      changed: version?.changed,
      slackUserId: user?.slackUserId,
      openComments,
      comment:
        comment && thread
          ? {
              id: comment._id,
              threadId: thread._id,
              body: comment.body,
              isReply: comment.parentId !== undefined,
              page: thread.page,
              pageTitle: pageTitle(thread.page),
              quote: thread.anchor?.text,
              threadAuthor: thread.author,
              threadBody: thread.body,
              type: thread.type,
            }
          : undefined,
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
