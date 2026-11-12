import { NextResponse } from "next/server";
import { BCS_COOKIE, sessionCookie } from "@/lib/bcs-auth";

/** Ends the BCS session, as signing out of the app does. */
export async function POST() {
  const response = NextResponse.json({ ok: true });
  response.cookies.set(BCS_COOKIE, "", { ...sessionCookie, maxAge: 0 });
  return response;
}
