import { NextRequest, NextResponse } from "next/server";
import { ConvexHttpClient } from "convex/browser";
import { api } from "../../../../../../../convex/_generated/api";
import type { Id } from "../../../../../../../convex/_generated/dataModel";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

function jsonResponse(body: unknown, status = 200): NextResponse {
  return NextResponse.json(body, {
    status,
    headers: {
      ...CORS_HEADERS,
      "Cache-Control": "no-store",
    },
  });
}

export async function OPTIONS(): Promise<Response> {
  return new Response(null, {
    status: 204,
    headers: CORS_HEADERS,
  });
}

interface RawComment {
  _id: Id<"comments">;
  bepId: Id<"beps">;
  versionId?: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  authorId: Id<"users">;
  authorName: string;
  parentId?: Id<"comments">;
  rootCommentId?: Id<"comments">;
  type: string;
  content: string;
  anchor?: {
    nodeId: string;
    nodeType: string;
    nodeText: string;
  };
  resolved: boolean;
  resolvedByName?: string;
  resolvedAt?: number;
  createdAt: number;
  updatedAt: number;
  pageName?: string;
  pageSlug?: string;
  versionNumber?: number;
  parentAuthorName?: string;
}

interface BepPage {
  _id: Id<"bepPages">;
  slug: string;
  title: string;
  content: string;
}

interface BepVersion {
  _id: Id<"bepVersions">;
  version: number;
}

interface ExportComment {
  id: string;
  type: string;
  content: string;
  author: string;
  createdAt: string;
  resolved: boolean;
  resolvedBy?: string;
  location: {
    page: string;
    pageSlug: string | null;
  };
  anchor?: {
    type: string;
    text: string;
    context?: string;
  };
  replies: ExportReply[];
}

interface ExportReply {
  id: string;
  content: string;
  author: string;
  createdAt: string;
}

function extractMarkdownContext(
  content: string | undefined,
  anchorText: string,
  contextLines: number = 3
): string | null {
  if (!content || !anchorText) return null;

  const lines = content.split("\n");
  const anchorTextLower = anchorText.toLowerCase().trim();

  let matchLineIndex = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].toLowerCase().includes(anchorTextLower.slice(0, 50))) {
      matchLineIndex = i;
      break;
    }
  }

  if (matchLineIndex === -1) {
    const words = anchorTextLower.split(/\s+/).filter((w) => w.length > 3);
    for (let i = 0; i < lines.length; i++) {
      const lineLower = lines[i].toLowerCase();
      const matchCount = words.filter((w) => lineLower.includes(w)).length;
      if (matchCount >= Math.min(3, words.length)) {
        matchLineIndex = i;
        break;
      }
    }
  }

  if (matchLineIndex === -1) return null;

  const startLine = Math.max(0, matchLineIndex - contextLines);
  const endLine = Math.min(lines.length, matchLineIndex + contextLines + 1);

  return lines.slice(startLine, endLine).join("\n");
}

function formatBepNumber(num: number): string {
  return `BEP-${String(num).padStart(3, "0")}`;
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ number: string }> }
): Promise<Response> {
  const convexUrl = process.env.NEXT_PUBLIC_CONVEX_URL;
  if (!convexUrl) {
    return jsonResponse(
      { error: "Missing NEXT_PUBLIC_CONVEX_URL environment variable." },
      500
    );
  }

  const resolvedParams = await params;
  const bepNumber = parseInt(resolvedParams.number, 10);
  if (isNaN(bepNumber)) {
    return jsonResponse({ error: "Invalid BEP number." }, 400);
  }

  const convex = new ConvexHttpClient(convexUrl);
  const searchParams = request.nextUrl.searchParams;
  const includeResolved = searchParams.get("includeResolved") === "true";
  const format = (searchParams.get("format") ?? "json").toLowerCase();

  try {
    // Use getByNumber which fetches everything including pages and versions
    const bepData = await convex.query(api.beps.getByNumber, { number: bepNumber });

    if (!bepData) {
      return jsonResponse(
        { error: `${formatBepNumber(bepNumber)} not found.` },
        404
      );
    }

    // Get the current version (first in the desc-sorted list)
    const versions = bepData.versions as BepVersion[];
    const currentVersion = versions.length > 0 ? versions[0] : null;

    if (!currentVersion) {
      return jsonResponse(
        { error: `No version found for ${formatBepNumber(bepNumber)}.` },
        404
      );
    }

    const rawComments = (await convex.query(api.comments.allByBepNewestFirst, {
      bepId: bepData._id,
      versionId: currentVersion._id,
      includeResolved,
    })) as RawComment[];

    const pages = bepData.pages as BepPage[];

    const pageContentMap: Record<string, string> = {
      _main: bepData.content ?? "",
    };
    for (const page of pages) {
      pageContentMap[page.slug] = page.content;
    }

    const rootComments = rawComments.filter((c) => !c.parentId);
    const repliesByParent = new Map<string, RawComment[]>();

    for (const comment of rawComments) {
      if (comment.parentId) {
        const parentId = String(comment.parentId);
        const existing = repliesByParent.get(parentId) || [];
        existing.push(comment);
        repliesByParent.set(parentId, existing);
      }
    }

    const exportComments: ExportComment[] = rootComments.map((comment) => {
      const pageSlug = comment.pageSlug ?? null;
      const pageKey = pageSlug ?? "_main";
      const pageContent = pageContentMap[pageKey];

      let anchorContext: string | undefined;
      if (comment.anchor && pageContent) {
        const context = extractMarkdownContext(
          pageContent,
          comment.anchor.nodeText
        );
        if (context) {
          anchorContext = context;
        }
      }

      const replies = repliesByParent.get(String(comment._id)) || [];
      replies.sort((a, b) => a.createdAt - b.createdAt);

      return {
        id: String(comment._id),
        type: comment.type,
        content: comment.content,
        author: comment.authorName,
        createdAt: new Date(comment.createdAt).toISOString(),
        resolved: comment.resolved,
        resolvedBy: comment.resolvedByName,
        location: {
          page: comment.pageName ?? "README",
          pageSlug,
        },
        anchor: comment.anchor
          ? {
              type: comment.anchor.nodeType,
              text: comment.anchor.nodeText,
              ...(anchorContext ? { context: anchorContext } : {}),
            }
          : undefined,
        replies: replies.map((reply) => ({
          id: String(reply._id),
          content: reply.content,
          author: reply.authorName,
          createdAt: new Date(reply.createdAt).toISOString(),
        })),
      };
    });

    const stats = {
      total: rootComments.length,
      byType: {
        discussion: rootComments.filter((c) => c.type === "discussion").length,
        concern: rootComments.filter((c) => c.type === "concern").length,
        question: rootComments.filter((c) => c.type === "question").length,
      },
      resolved: rootComments.filter((c) => c.resolved).length,
      unresolved: rootComments.filter((c) => !c.resolved).length,
      totalReplies: rawComments.filter((c) => c.parentId).length,
    };

    if (format === "markdown") {
      const markdown = generateCommentsMarkdown(
        bepNumber,
        bepData.title,
        currentVersion.version,
        exportComments,
        stats
      );
      return new Response(markdown, {
        status: 200,
        headers: {
          ...CORS_HEADERS,
          "Cache-Control": "no-store",
          "Content-Type": "text/markdown; charset=utf-8",
        },
      });
    }

    const origin = request.nextUrl.origin;
    return jsonResponse({
      bep: {
        number: bepNumber,
        id: formatBepNumber(bepNumber),
        title: bepData.title,
        version: currentVersion.version,
      },
      stats,
      comments: exportComments,
      usage: {
        reply: `POST ${origin}/api/agent/beps/${bepNumber}/comments/reply`,
        replyBody: {
          commentId: "<comment-id>",
          content: "<your reply>",
        },
      },
    });
  } catch (err) {
    return jsonResponse(
      {
        error: "Failed to fetch comments.",
        detail: err instanceof Error ? err.message : String(err),
      },
      502
    );
  }
}

function generateCommentsMarkdown(
  bepNumber: number,
  title: string,
  version: number,
  comments: ExportComment[],
  stats: {
    total: number;
    byType: { discussion: number; concern: number; question: number };
    resolved: number;
    unresolved: number;
    totalReplies: number;
  }
): string {
  const bepId = formatBepNumber(bepNumber);

  let md = `# Comments - ${bepId}: ${title}

> **Version:** ${version}
> **Total Comments:** ${stats.total} (${stats.unresolved} unresolved, ${stats.resolved} resolved)
> **Replies:** ${stats.totalReplies}

## Summary by Type

| Type | Count |
|------|-------|
| Discussion | ${stats.byType.discussion} |
| Concern | ${stats.byType.concern} |
| Question | ${stats.byType.question} |

---

`;

  const unresolvedConcerns = comments.filter(
    (c) => c.type === "concern" && !c.resolved
  );
  if (unresolvedConcerns.length > 0) {
    md += `## ⚠️ Unresolved Concerns (${unresolvedConcerns.length})

These require attention before the BEP can progress.

`;
    for (const comment of unresolvedConcerns) {
      md += formatCommentMarkdown(comment);
    }
  }

  const unresolvedQuestions = comments.filter(
    (c) => c.type === "question" && !c.resolved
  );
  if (unresolvedQuestions.length > 0) {
    md += `## ❓ Unanswered Questions (${unresolvedQuestions.length})

`;
    for (const comment of unresolvedQuestions) {
      md += formatCommentMarkdown(comment);
    }
  }

  const unresolvedDiscussions = comments.filter(
    (c) => c.type === "discussion" && !c.resolved
  );
  if (unresolvedDiscussions.length > 0) {
    md += `## 💬 Open Discussions (${unresolvedDiscussions.length})

`;
    for (const comment of unresolvedDiscussions) {
      md += formatCommentMarkdown(comment);
    }
  }

  const resolved = comments.filter((c) => c.resolved);
  if (resolved.length > 0) {
    md += `## ✅ Resolved (${resolved.length})

<details>
<summary>View resolved comments</summary>

`;
    for (const comment of resolved) {
      md += formatCommentMarkdown(comment);
    }
    md += `</details>

`;
  }

  return md;
}

function formatCommentMarkdown(comment: ExportComment): string {
  const typeEmoji =
    comment.type === "concern"
      ? "⚠️"
      : comment.type === "question"
        ? "❓"
        : "💬";

  let md = `### ${typeEmoji} ${comment.author} on ${comment.location.page}

**ID:** \`${comment.id}\`
**Date:** ${comment.createdAt}
${comment.resolved ? `**Status:** ✅ Resolved by ${comment.resolvedBy}` : "**Status:** Open"}

`;

  if (comment.anchor) {
    md += `**Referenced text:**
> ${comment.anchor.text.slice(0, 200)}${comment.anchor.text.length > 200 ? "..." : ""}

`;
    if (comment.anchor.context) {
      md += `**Context (surrounding markdown):**
\`\`\`markdown
${comment.anchor.context}
\`\`\`

`;
    }
  }

  md += `**Comment:**
${comment.content}

`;

  if (comment.replies.length > 0) {
    md += `**Replies (${comment.replies.length}):**

`;
    for (const reply of comment.replies) {
      md += `> **${reply.author}** (${reply.createdAt}):
> ${reply.content.split("\n").join("\n> ")}

`;
    }
  }

  md += `---

`;

  return md;
}
