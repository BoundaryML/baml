// The signed-in member's API token, for agents and the `bep` script. Tokens are only for BoundaryML
// organization members, which the GitHub callback checked and recorded in the signed BCS cookie:
// the browser cannot ask Convex for a token itself.
import { NextResponse } from "next/server";
import { api } from "../../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall } from "@/lib/bcs-convex";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

function json(body: unknown, status = 200) {
  return NextResponse.json(body, { status, headers: { "Cache-Control": "no-store" } });
}

async function token(regenerate: boolean) {
  const viewer = await currentViewer();
  if (!viewer) {
    return json(
      {
        error:
          "API tokens are for BoundaryML organization members: sign out and sign in again with GitHub.",
      },
      403
    );
  }
  const result = await bcsCall((convex, secret) =>
    convex.mutation(api.users.apiTokenFor, { secret, githubId: viewer.githubId, regenerate })
  );
  if (!result.ok) {
    return json({ error: result.failure.message }, result.failure.kind === "missing" ? 404 : 503);
  }
  return json(result.value);
}

/** The current token, made on first use. */
export async function GET() {
  return token(false);
}

/** A new token, which replaces the current one. */
export async function POST() {
  return token(true);
}
