// Who signed in with GitHub, for BEP API tokens. Server only.
//
// The GitHub callback is the one place the server knows the viewer's real GitHub account; there it
// sets this signed cookie for everyone who signs in. Only /api/token reads it: BEP API tokens go
// to BoundaryML organization members (the BCS cookie, src/lib/bcs-auth.ts) and to the
// collaborators outside the organization listed in BEPS_TOKEN_GUESTS. This cookie never grants
// access to BCS proposals: it is signed for a different use, and nothing in BCS reads it.
import { createHmac, timingSafeEqual } from "node:crypto";
import { cookies } from "next/headers";

export const USER_COOKIE = "beps_user";
const USER_SECONDS = 14 * 24 * 60 * 60;
const USE = "beps-user";

export type BepsUser = { githubId: string; login: string };

export const userCookie = {
  httpOnly: true,
  secure: process.env.NODE_ENV === "production",
  sameSite: "lax" as const,
  path: "/",
  maxAge: USER_SECONDS,
};

function secret(): string | null {
  const value = process.env.BCS_SESSION_SECRET?.trim();
  return value && value.length >= 32 ? value : null;
}

function mac(payload: string, key: string): string {
  return createHmac("sha256", key).update(payload).digest("base64url");
}

/** The cookie's value for a GitHub account the callback just verified, or null when not configured. */
export function signUser(user: BepsUser): string | null {
  const key = secret();
  if (!key) return null;
  const payload = Buffer.from(
    JSON.stringify({ ...user, use: USE, exp: Math.floor(Date.now() / 1000) + USER_SECONDS })
  ).toString("base64url");
  return `${payload}.${mac(payload, key)}`;
}

export function verifyUser(token: string | undefined): BepsUser | null {
  const key = secret();
  if (!key || !token) return null;
  const [payload, signature] = token.split(".");
  if (!payload || !signature) return null;
  const expected = Buffer.from(mac(payload, key));
  const given = Buffer.from(signature);
  if (expected.length !== given.length || !timingSafeEqual(expected, given)) return null;
  try {
    const claims = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
    if (claims.use !== USE) return null;
    if (typeof claims.exp !== "number" || claims.exp < Date.now() / 1000) return null;
    if (typeof claims.githubId !== "string" || typeof claims.login !== "string") return null;
    return { githubId: claims.githubId, login: claims.login };
  } catch {
    return null;
  }
}

/** The GitHub account signed in on this request, from the callback's cookie. */
export async function currentUser(): Promise<BepsUser | null> {
  const jar = await cookies();
  return verifyUser(jar.get(USER_COOKIE)?.value);
}

/**
 * Whether `login` is a collaborator outside the BoundaryML organization who may get a BEP API
 * token: BEPS_TOKEN_GUESTS lists their GitHub logins, comma-separated.
 */
export function isTokenGuest(login: string): boolean {
  const guests = (process.env.BEPS_TOKEN_GUESTS ?? "")
    .split(",")
    .map((each) => each.trim().replace(/^@/, "").toLowerCase())
    .filter(Boolean);
  return guests.includes(login.toLowerCase());
}
