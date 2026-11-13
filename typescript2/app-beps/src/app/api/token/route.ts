// The signed-in member's API token, for agents and the `bep` script. Tokens are for BoundaryML
// organization members, which the GitHub callback checked and recorded in the signed BCS cookie,
// and for the collaborators outside the organization listed in BEPS_TOKEN_GUESTS, whose GitHub
// account the callback recorded in its own signed cookie: the browser cannot ask Convex for a
// token itself. A BEP API token does not open BCS proposals.
import { NextResponse } from "next/server";
import { api } from "../../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { currentUser, isTokenGuest } from "@/lib/beps-user";
import { bcsCall } from "@/lib/bcs-convex";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

function json(body: unknown, status = 200) {
  return NextResponse.json(body, { status, headers: { "Cache-Control": "no-store" } });
}

/** The GitHub account a token may be issued to: an org member, or a listed guest. */
async function holder(): Promise<{ githubId: string } | null> {
  const member = await currentViewer();
  if (member) return member;
  const user = await currentUser();
  return user && isTokenGuest(user.login) ? user : null;
}

async function token(regenerate: boolean) {
  const viewer = await holder();
  if (!viewer) {
    return json(
      {
        error:
          "API tokens are for BoundaryML organization members and invited collaborators: sign out and sign in again with GitHub, or ask the team to add your GitHub login.",
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
