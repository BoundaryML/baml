// A proposal's comment threads, for agents and `bep bcs comments`, as BEP agents read a BEP's:
// every thread with its replies, where it is, and for a thread on a block of a Markdown page the
// block's Markdown source in the current version, so an agent can find what to change.
import { NextRequest, NextResponse } from "next/server";
import { api } from "../../../../../../../convex/_generated/api";
import { bcsCall, bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { renderBcsPage } from "@/lib/bcs-render";
import { agentName } from "../../../../../../../convex/bcsAgents";
import { authenticate, json, refused } from "../../shared";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = { params: Promise<{ number: string }> };

type Reply = {
  id: string;
  author: string;
  /** The coding agent that wrote it for the author, when one said so. */
  agent?: string;
  createdAt: string;
  editedAt?: string;
  body: string;
};

type AgentThread = {
  id: string;
  type: "discussion" | "concern" | "question";
  author: string;
  /** The coding agent that wrote it for the author, when one said so. */
  agent?: string;
  createdAt: string;
  editedAt?: string;
  /** Its first comment was deleted; its replies stay. */
  deleted: boolean;
  resolved: boolean;
  resolvedBy?: string;
  page: { slug: string; title: string };
  url: string;
  /**
   * The block it is on: the text quoted when it was started, and the block's Markdown source in
   * the current version, null when that version no longer has the block (its text changed).
   */
  anchor?: { block: string; quote: string; source: string | null };
  body: string;
  reactions: Record<string, string[]>;
  replies: Reply[];
};

function iso(time: number | undefined): string | undefined {
  return time === undefined ? undefined : new Date(time).toISOString();
}

export async function GET(request: NextRequest, { params }: Params) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const number = Number((await params).number);
  if (!Number.isInteger(number)) return json({ error: "No such proposal." }, 404);
  const includeResolved = request.nextUrl.searchParams.get("includeResolved") === "true";
  const format = request.nextUrl.searchParams.get("format") === "markdown" ? "markdown" : "json";

  const found = await bcsCall(async (convex, secret) => {
    const [proposal, threads] = await Promise.all([
      convex.query(api.bcs.getAll, { secret, number }),
      convex.query(api.bcsComments.threads, { secret, number }),
    ]);
    return { proposal, threads };
  });
  if (!found.ok) return refused(found.failure);
  const { proposal, threads } = found.value;
  if (!proposal || !threads) return json({ error: "No such proposal." }, 404);

  // Each Markdown page's blocks, rendered as the reader renders the current version (its links
  // included, which name a block with no text), so block ids match.
  const sources = new Map<string, Record<string, string>>();
  for (const page of proposal.pages) {
    if (page.format !== "markdown") continue;
    const rendered = await renderBcsPage({
      ...page,
      pageHref: (slug) => bcsPagePath(number, slug),
    });
    sources.set(page.slug, rendered.sources);
  }
  const titles = new Map(proposal.pages.map((page) => [page.slug, page.title]));
  const origin = request.nextUrl.origin;
  const comments: AgentThread[] = threads
    .filter((thread) => includeResolved || !thread.resolved)
    .map((thread) => ({
      id: thread.id,
      type: thread.type,
      author: thread.author,
      agent: thread.agent,
      createdAt: iso(thread.createdAt)!,
      editedAt: iso(thread.editedAt),
      deleted: thread.deleted,
      resolved: thread.resolved,
      resolvedBy: thread.resolvedBy,
      page: { slug: thread.page, title: titles.get(thread.page) ?? thread.page },
      url: `${origin}/bcs/${number}?comment=${thread.id}`,
      anchor: thread.anchor && {
        block: thread.anchor.block,
        quote: thread.anchor.text,
        source: sources.get(thread.page)?.[thread.anchor.block] ?? null,
      },
      body: thread.body,
      reactions: Object.fromEntries(
        Object.entries(thread.reactions).filter((entry): entry is [string, string[]] => !!entry[1]?.length)
      ),
      replies: thread.replies.map((reply) => ({
        id: reply.id,
        author: reply.author,
        agent: reply.agent,
        createdAt: iso(reply.createdAt)!,
        editedAt: iso(reply.editedAt),
        body: reply.deleted ? "" : reply.body,
      })),
    }));

  const roots = threads.filter((thread) => includeResolved || !thread.resolved);
  const stats = {
    threads: roots.length,
    open: threads.filter((thread) => !thread.resolved).length,
    resolved: threads.filter((thread) => thread.resolved).length,
    replies: roots.reduce((sum, thread) => sum + thread.replies.length, 0),
  };
  const summary = { number, id: bcsId(number), title: proposal.title, version: proposal.version };

  if (format === "markdown") {
    return new NextResponse(commentsMarkdown(summary, stats, comments, includeResolved), {
      headers: { "Cache-Control": "no-store", "Content-Type": "text/markdown; charset=utf-8" },
    });
  }
  return json({
    proposal: { ...summary, url: `${origin}/bcs/${number}` },
    stats,
    comments,
    usage: {
      reply: `POST ${origin}/api/agent/bcs/${number}/comments/reply`,
      replyBody: {
        commentId: "<comment or reply id>",
        body: "<your reply, Markdown>",
        agent: "<optional: the agent writing for you, such as claude or codex>",
      },
    },
  });
}

function commentsMarkdown(
  proposal: { id: string; title: string; version: number },
  stats: { threads: number; open: number; resolved: number; replies: number },
  comments: AgentThread[],
  includeResolved: boolean
): string {
  const lines = [
    `# Comments on ${proposal.id}: ${proposal.title}`,
    "",
    `Version ${proposal.version}. ${stats.open} open thread(s), ${stats.resolved} resolved` +
      (includeResolved ? "." : " (not shown: pass --include-resolved)."),
    "",
  ];
  const groups: [string, AgentThread[]][] = [
    ["Open concerns", comments.filter((each) => !each.resolved && each.type === "concern")],
    ["Open questions", comments.filter((each) => !each.resolved && each.type === "question")],
    ["Open discussions", comments.filter((each) => !each.resolved && each.type === "discussion")],
    ["Resolved", comments.filter((each) => each.resolved)],
  ];
  for (const [heading, group] of groups) {
    if (group.length === 0) continue;
    lines.push(`## ${heading} (${group.length})`, "");
    for (const thread of group) lines.push(...threadMarkdown(thread));
  }
  if (comments.length === 0) lines.push("No comments.", "");
  return lines.join("\n");
}

function quoted(text: string): string {
  return text
    .split("\n")
    .map((line) => (line ? `> ${line}` : ">"))
    .join("\n");
}

/** Who wrote a comment: the member, or their agent. */
function byline(comment: { author: string; agent?: string }): string {
  return comment.agent ? `${comment.author}'s agent ${agentName(comment.agent)}` : comment.author;
}

function threadMarkdown(thread: AgentThread): string[] {
  const where = thread.page.slug === "" ? "the main page" : `page "${thread.page.title}" (${thread.page.slug})`;
  const lines = [
    `### ${thread.deleted ? "(deleted)" : byline(thread)}, on ${where}`,
    "",
    `- ID: \`${thread.id}\``,
    `- Kind: ${thread.type}${thread.resolved ? `, resolved by ${thread.resolvedBy ?? "someone"}` : ", open"}`,
    `- Started: ${thread.createdAt}${thread.editedAt ? ` (edited ${thread.editedAt})` : ""}`,
    `- Link: ${thread.url}`,
    "",
  ];
  if (thread.anchor) {
    lines.push("On the block:", "", quoted(thread.anchor.quote), "");
    if (thread.anchor.source !== null) {
      // A fence longer than any backtick run inside, so the source cannot close it.
      const fence = "`".repeat(Math.max(3, ...[...thread.anchor.source.matchAll(/`+/g)].map((m) => m[0].length + 1)));
      lines.push("Its Markdown source now:", "", `${fence}markdown`, thread.anchor.source, fence, "");
    } else {
      lines.push("(The current version no longer has this block: its text changed.)", "");
    }
  }
  lines.push(thread.deleted ? "(The first comment was deleted.)" : thread.body, "");
  for (const reply of thread.replies) {
    lines.push(
      `**${byline(reply)}** replied (${reply.createdAt}, ID \`${reply.id}\`):`,
      "",
      quoted(reply.body || "(deleted)"),
      ""
    );
  }
  lines.push("---", "");
  return lines;
}
