import { NextRequest, NextResponse } from "next/server";
import { cookies } from "next/headers";
import { challenge, cookieOptions, nonce, runPath, issuePath, proposalPath, seal, siteOrigin, STATE_COOKIE } from "@/lib/approval-auth";

export async function GET(request: NextRequest) {
  try {
    const id = request.nextUrl.searchParams.get("proposal") ?? "";
    const issue = request.nextUrl.searchParams.get("issue");
    const run = request.nextUrl.searchParams.get("run");
    const returnTo = run !== null ? runPath(run) : issue !== null ? issuePath(issue) : proposalPath(id);
    const clientId = process.env.FEEDBACK_GITHUB_CLIENT_ID;
    if (!clientId) throw new Error("OAuth not configured");
    const state = nonce(), verifier = nonce();
    (await cookies()).set(STATE_COOKIE, seal({ state, verifier, returnTo, expires: Date.now() + 600000 }),
      { ...cookieOptions, maxAge: 600 });
    const url = new URL("https://github.com/login/oauth/authorize");
    url.search = new URLSearchParams({ client_id: clientId, redirect_uri: `${siteOrigin()}/auth/github/callback`,
      scope: "read:user", state, code_challenge: challenge(verifier), code_challenge_method: "S256" }).toString();
    return NextResponse.redirect(url);
  } catch {
    return new NextResponse("Website approval sign-in is unavailable. You can approve the proposal in Slack.", { status: 503 });
  }
}
