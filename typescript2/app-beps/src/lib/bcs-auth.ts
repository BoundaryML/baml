// Who may read BCS design proposals, and the signed cookie that says so.
//
// The GitHub login callback is the one place the server knows the viewer's real GitHub account
// (the rest of the app trusts what the browser says). There, `isBcsMember` decides, and a member
// gets a cookie signed with BCS_SESSION_SECRET that pages and routes check on the server.
//
// Until the callback checks membership of the BoundaryML organization on GitHub, the team is the
// accounts listed below, or any account with a verified @boundaryml.com email on GitHub.
import { createHmac, timingSafeEqual } from "node:crypto";
import { cookies } from "next/headers";

export const BCS_COOKIE = "bcs_session";
const MAX_AGE_SECONDS = 30 * 24 * 60 * 60;
/** How long a CLI token works, after which its owner gets a new one from /bcs/token. */
const TOKEN_SECONDS = 90 * 24 * 60 * 60;
/** What a signed value is for: the browser's cookie, or the `bep bcs` CLI's bearer token. */
type Use = "session" | "cli";
const TOKEN_PREFIX = "bcs_";

/** BoundaryML's members on GitHub, by account ID, which a rename keeps. */
const TEAM: Record<string, string> = {
  "22033723": "2kai2kai2",
  "5353992": "aaronvg",
  "54243067": "antoniosarosi",
  "17539023": "codeshaunted",
  "2448742": "hellovai",
  "82104712": "rossirpaulo",
  "512410": "sxlijin",
};

export type BcsViewer = { githubId: string; login: string };

/** Whether a GitHub account may read BCS proposals. */
export function isBcsMember(githubId: string, verifiedEmails: string[]): boolean {
  return (
    githubId in TEAM ||
    verifiedEmails.some((email) => email.toLowerCase().endsWith("@boundaryml.com"))
  );
}

function secret(): string | null {
  const value = process.env.BCS_SESSION_SECRET;
  return value && value.length >= 32 ? value : null;
}

function mac(payload: string, key: string): string {
  return createHmac("sha256", key).update(payload).digest("base64url");
}

function sign(viewer: BcsViewer, use: Use, seconds: number): string | null {
  const key = secret();
  if (!key) return null;
  const payload = Buffer.from(
    JSON.stringify({ ...viewer, use, exp: Math.floor(Date.now() / 1000) + seconds })
  ).toString("base64url");
  return `${payload}.${mac(payload, key)}`;
}

/** The cookie's value for `viewer`, or null when BCS proposals are not configured. */
export function signSession(viewer: BcsViewer): string | null {
  return sign(viewer, "session", MAX_AGE_SECONDS);
}

/** A `bep bcs` CLI token for `viewer`, which /bcs/token shows to a signed-in member. */
export function signCliToken(viewer: BcsViewer): string | null {
  const signed = sign(viewer, "cli", TOKEN_SECONDS);
  return signed && `${TOKEN_PREFIX}${signed}`;
}

/** The viewer a cookie names, when it is ours and has not expired. */
export function verifySession(token: string | undefined): BcsViewer | null {
  return verify(token, "session");
}

/** The member a request's `Authorization: Bearer bcs_...` names, when the token is ours. */
export function verifyCliRequest(headers: Headers): BcsViewer | null {
  const bearer = headers.get("authorization")?.match(/^Bearer (.+)$/)?.[1];
  if (!bearer?.startsWith(TOKEN_PREFIX)) return null;
  return verify(bearer.slice(TOKEN_PREFIX.length), "cli");
}

function verify(token: string | undefined, use: Use): BcsViewer | null {
  const key = secret();
  if (!key || !token) return null;
  const [payload, signature] = token.split(".");
  if (!payload || !signature) return null;
  const expected = Buffer.from(mac(payload, key));
  const given = Buffer.from(signature);
  if (expected.length !== given.length || !timingSafeEqual(expected, given)) return null;
  try {
    const claims = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
    if (claims.use !== use) return null;
    if (typeof claims.exp !== "number" || claims.exp < Date.now() / 1000) return null;
    if (typeof claims.githubId !== "string" || typeof claims.login !== "string") return null;
    return { githubId: claims.githubId, login: claims.login };
  } catch {
    return null;
  }
}

export const sessionCookie = {
  httpOnly: true,
  secure: process.env.NODE_ENV === "production",
  sameSite: "lax" as const,
  path: "/",
  maxAge: MAX_AGE_SECONDS,
};

/**
 * The signed-in team member viewing this request, or null. In development, BCS_DEV_VIEWER (a
 * GitHub login) stands in, since GitHub's login returns to the production site.
 */
export async function currentViewer(): Promise<BcsViewer | null> {
  if (process.env.NODE_ENV !== "production" && process.env.BCS_DEV_VIEWER) {
    return { githubId: "dev", login: process.env.BCS_DEV_VIEWER };
  }
  const jar = await cookies();
  return verifySession(jar.get(BCS_COOKIE)?.value);
}
