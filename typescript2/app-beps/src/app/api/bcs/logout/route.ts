import { NextResponse } from "next/server";
import { BCS_COOKIE, sessionCookie } from "@/lib/bcs-auth";
import { USER_COOKIE, userCookie } from "@/lib/beps-user";

/** Ends the BCS session and the GitHub sign-in cookie, as signing out of the app does. */
export async function POST() {
  const response = NextResponse.json({ ok: true });
  response.cookies.set(BCS_COOKIE, "", { ...sessionCookie, maxAge: 0 });
  response.cookies.set(USER_COOKIE, "", { ...userCookie, maxAge: 0 });
  return response;
}
