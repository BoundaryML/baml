import { randomBytes } from "node:crypto";
import { NextResponse } from "next/server";
import { siteOrigin } from "@/lib/auth";

export async function GET() {
  if (!process.env.ATB2_GITHUB_CLIENT_ID || !process.env.ATB2_GITHUB_CLIENT_SECRET || !process.env.ATB2_UI_SESSION_SECRET) {
    return NextResponse.json({ error: "GitHub sign-in has not been configured." }, { status: 503 });
  }
  const state = randomBytes(32).toString("hex");
  const url = new URL("https://github.com/login/oauth/authorize");
  url.search = new URLSearchParams({ client_id: process.env.ATB2_GITHUB_CLIENT_ID, redirect_uri: siteOrigin() + "/api/auth/github/callback", scope: "read:org", state }).toString();
  const response = NextResponse.redirect(url);
  response.cookies.set("atb2-oauth-state", state, { httpOnly: true, secure: true, sameSite: "lax", path: "/api/auth/github", maxAge: 600 });
  response.headers.set("Cache-Control", "no-store");
  return response;
}
