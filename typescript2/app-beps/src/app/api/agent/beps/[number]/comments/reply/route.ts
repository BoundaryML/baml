import { NextRequest, NextResponse } from "next/server";
import { ConvexHttpClient } from "convex/browser";
import { api } from "../../../../../../../../convex/_generated/api";
import type { Id } from "../../../../../../../../convex/_generated/dataModel";

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

interface AuthenticatedUser {
  _id: string;
  name: string;
  role: string;
}

async function authenticateRequest(
  request: NextRequest,
  convex: ConvexHttpClient
): Promise<{ user: AuthenticatedUser } | { error: string; status: number }> {
  const authHeader = request.headers.get("Authorization");

  if (!authHeader) {
    return {
      error: "Missing Authorization header. Use 'Authorization: Bearer <token>'.",
      status: 401,
    };
  }

  if (!authHeader.startsWith("Bearer ")) {
    return {
      error: "Invalid Authorization header format. Use 'Authorization: Bearer <token>'.",
      status: 401,
    };
  }

  const token = authHeader.slice(7); // Remove "Bearer " prefix

  if (!token) {
    return { error: "Empty token.", status: 401 };
  }

  try {
    const user = await convex.query(api.users.authenticateWithToken, { token });

    if (!user) {
      return { error: "Invalid or expired token.", status: 401 };
    }

    return { user };
  } catch {
    return {
      error: "Authentication failed.",
      status: 500,
    };
  }
}

function formatBepNumber(num: number): string {
  return `BEP-${String(num).padStart(3, "0")}`;
}

interface BepVersion {
  _id: Id<"bepVersions">;
  version: number;
}

interface ReplyRequest {
  commentId: string;
  content: string;
  type?: "discussion" | "concern" | "question";
}

export async function POST(
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

  // Authenticate via Bearer token
  const authResult = await authenticateRequest(request, convex);
  if ("error" in authResult) {
    return jsonResponse({ error: authResult.error }, authResult.status);
  }
  const { user } = authResult;

  let body: ReplyRequest;
  try {
    body = await request.json();
  } catch {
    return jsonResponse({ error: "Invalid JSON body." }, 400);
  }

  // Validate required fields
  if (!body.commentId || typeof body.commentId !== "string") {
    return jsonResponse({ error: "Missing or invalid 'commentId' field." }, 400);
  }
  if (!body.content || typeof body.content !== "string") {
    return jsonResponse({ error: "Missing or invalid 'content' field." }, 400);
  }

  const trimmedContent = body.content.trim();
  if (trimmedContent.length === 0) {
    return jsonResponse({ error: "Content cannot be empty." }, 400);
  }

  try {
    // Fetch the BEP to verify it exists
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

    // Verify the parent comment exists and belongs to this BEP
    const allComments = await convex.query(api.comments.allByBepNewestFirst, {
      bepId: bepData._id,
      versionId: currentVersion._id,
      includeResolved: true,
    });

    const parentComment = allComments.find(
      (c) => String(c._id) === body.commentId
    );

    if (!parentComment) {
      return jsonResponse(
        {
          error: `Comment with ID '${body.commentId}' not found in ${formatBepNumber(bepNumber)} v${currentVersion.version}.`,
          hint: "The commentId must be from the current version of the BEP.",
        },
        404
      );
    }

    // Use the root comment if replying to a nested reply
    const rootCommentId = parentComment.parentId ?? parentComment._id;

    // Add the reply comment
    const commentId = await convex.mutation(api.comments.add, {
      bepId: bepData._id,
      versionId: currentVersion._id,
      pageId: parentComment.pageId,
      parentId: rootCommentId,
      authorId: user._id as Id<"users">,
      type: body.type ?? "discussion",
      content: trimmedContent,
      // Include anchor so reply appears in context
      anchor: parentComment.anchor,
    });

    const origin = request.nextUrl.origin;
    return jsonResponse({
      success: true,
      commentId: String(commentId),
      repliedTo: {
        commentId: body.commentId,
        author: parentComment.authorName,
      },
      author: user.name,
      bep: {
        number: bepNumber,
        id: formatBepNumber(bepNumber),
        version: currentVersion.version,
      },
      viewUrl: `${origin}/beps/${bepNumber}`,
    });
  } catch (err) {
    return jsonResponse(
      {
        error: "Failed to add reply.",
        detail: err instanceof Error ? err.message : String(err),
      },
      500
    );
  }
}

// Also support GET to show usage instructions
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ number: string }> }
): Promise<Response> {
  const resolvedParams = await params;
  const bepNumber = parseInt(resolvedParams.number, 10);
  const origin = request.nextUrl.origin;

  return jsonResponse({
    endpoint: `POST ${origin}/api/agent/beps/${bepNumber}/comments/reply`,
    description: "Reply to a comment on this BEP",
    authentication: {
      method: "Bearer token",
      header: "Authorization: Bearer <your-api-token>",
      tokenSource: `Get your API token from ${origin}/profile`,
    },
    requestBody: {
      commentId: {
        type: "string",
        required: true,
        description: "The ID of the comment to reply to (from the comments endpoint)",
      },
      content: {
        type: "string",
        required: true,
        description: "The markdown content of your reply",
      },
      type: {
        type: "string",
        required: false,
        default: "discussion",
        options: ["discussion", "concern", "question"],
        description: "The type of comment",
      },
    },
    example: {
      curl: `curl -X POST "${origin}/api/agent/beps/${bepNumber}/comments/reply" \\
  -H "Authorization: Bearer <your-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "commentId": "<comment-id-from-comments-endpoint>",
    "content": "Your reply here"
  }'`,
    },
    relatedEndpoints: {
      getComments: `GET ${origin}/api/agent/beps/${bepNumber}/comments`,
    },
  });
}
