import { NextRequest, NextResponse } from "next/server";
import { ConvexHttpClient } from "convex/browser";
import { api } from "../../../../../convex/_generated/api";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
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

// ─────────────────────────────────────────────────────────────────────────────
// GET - List users (for finding shepherd IDs)
// ─────────────────────────────────────────────────────────────────────────────

export async function GET(): Promise<Response> {
  const convexUrl = process.env.NEXT_PUBLIC_CONVEX_URL;
  if (!convexUrl) {
    return jsonResponse(
      { error: "Missing NEXT_PUBLIC_CONVEX_URL environment variable." },
      500
    );
  }

  const convex = new ConvexHttpClient(convexUrl);

  try {
    const users = await convex.query(api.users.list, {});

    return jsonResponse({
      users: users.map((user) => ({
        id: user._id,
        name: user.name,
        role: user.role,
      })),
      usage: {
        authenticate: "POST /api/agent/users with { name, passkey }",
        listUsers: "GET /api/agent/users",
      },
    });
  } catch (err) {
    return jsonResponse(
      {
        error: "Failed to fetch users.",
        detail: err instanceof Error ? err.message : String(err),
      },
      502
    );
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST - Authenticate / get or create user
// ─────────────────────────────────────────────────────────────────────────────

interface AuthRequest {
  name: string;
  passkey: string;
}

export async function POST(request: NextRequest): Promise<Response> {
  const convexUrl = process.env.NEXT_PUBLIC_CONVEX_URL;
  if (!convexUrl) {
    return jsonResponse(
      { error: "Missing NEXT_PUBLIC_CONVEX_URL environment variable." },
      500
    );
  }

  const convex = new ConvexHttpClient(convexUrl);

  let body: AuthRequest;
  try {
    body = await request.json();
  } catch {
    return jsonResponse({ error: "Invalid JSON body." }, 400);
  }

  // Validate required fields
  if (!body.name || typeof body.name !== "string") {
    return jsonResponse({ error: "Missing or invalid 'name' field." }, 400);
  }
  if (!body.passkey || typeof body.passkey !== "string") {
    return jsonResponse({ error: "Missing or invalid 'passkey' field." }, 400);
  }

  try {
    // getOrCreate returns just the user ID
    const userId = await convex.mutation(api.users.getOrCreate, {
      name: body.name,
      passkey: body.passkey,
    });

    return jsonResponse({
      userId: userId,
      name: body.name,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);

    // Check for invalid passkey error
    if (message.includes("Invalid passkey")) {
      return jsonResponse({ error: "Invalid passkey." }, 401);
    }

    return jsonResponse(
      {
        error: "Authentication failed.",
        detail: message,
      },
      500
    );
  }
}
