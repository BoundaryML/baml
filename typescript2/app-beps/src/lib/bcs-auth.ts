// Who may read BCS design proposals, and the signed cookie that says so.
//
// The GitHub login callback is the one place the server knows the viewer's real GitHub account
// (the rest of the app trusts what the browser says). There it asks GitHub whether the account is
// an active member of the BoundaryML organization, and a member gets a cookie signed with
// BCS_SESSION_SECRET that pages and routes check on the server.
//
// GitHub only answers with the viewer's own token, so membership is checked at sign-in, not on
// every request. The cookie and every CLI token made from it record when that check happened and
// expire two weeks after it, so someone removed from the organization keeps access for two weeks
// at most, however late in a session they make a CLI token.
import { createHmac, timingSafeEqual } from "node:crypto";
import { cookies } from "next/headers";

export const BCS_COOKIE = "bcs_session";
/** How long a membership check lasts: the cookie, and CLI tokens made from it, expire after it. */
const ACCESS_SECONDS = 14 * 24 * 60 * 60;
/** What a signed value is for: the browser's cookie, or the `bep bcs` CLI's bearer token. */
type Use = "session" | "cli";
const TOKEN_PREFIX = "bcs_";

/** The GitHub organization whose members may read BCS proposals. */
export const BCS_ORG = "BoundaryML";

/**
 * A team member: an active member of BCS_ORG at `checkedAt` (Unix seconds), when they signed in.
 * `via` names that check, so a cookie or token from an earlier rule (a hardcoded list, a verified
 * email) no longer counts.
 */
export type BcsViewer = { githubId: string; login: string; via: "org"; checkedAt: number };

/** When access granted by `viewer`'s membership check ends, in Unix seconds. */
export function accessEndsAt(viewer: BcsViewer): number {
  return viewer.checkedAt + ACCESS_SECONDS;
}

/**
 * Whether the account behind a GitHub OAuth token (with the read:org scope) is an active member of
 * BCS_ORG. GitHub answers 404 for non-members, and 403 when the organization has not approved this
 * OAuth app or requires an SSO session the token does not have; all of these mean no.
 */
export async function isBcsOrgMember(accessToken: string): Promise<boolean> {
  try {
    const response = await fetch(`https://api.github.com/user/memberships/orgs/${BCS_ORG}`, {
      headers: { Authorization: `Bearer ${accessToken}`, Accept: "application/vnd.github+json" },
      cache: "no-store",
      // A GitHub that stops answering must not hold up sign-in: no answer means no BCS access.
      signal: AbortSignal.timeout(5_000),
    });
    if (response.status === 403) {
      const why = await response.json().catch(() => ({}));
      console.warn(`GitHub refused the ${BCS_ORG} membership check:`, why?.message);
    }
    if (!response.ok) return false;
    const membership = (await response.json()) as { state?: string };
    return membership.state === "active";
  } catch (error) {
    console.warn(`The ${BCS_ORG} membership check failed:`, error);
    return false;
  }
}

function secret(): string | null {
  // Trimmed: a secret pasted into a dashboard often gains a trailing newline.
  const value = process.env.BCS_SESSION_SECRET?.trim();
  return value && value.length >= 32 ? value : null;
}

/** Whether this deployment can sign BCS sessions at all, so a sign-in can work. */
export function bcsSessionConfigured(): boolean {
  return secret() !== null;
}

function mac(payload: string, key: string): string {
  return createHmac("sha256", key).update(payload).digest("base64url");
}

/** Signs `viewer` for `use`, expiring with its membership check rather than from now. */
function sign(viewer: BcsViewer, use: Use): string | null {
  const key = secret();
  if (!key) return null;
  const payload = Buffer.from(
    JSON.stringify({ ...viewer, use, exp: accessEndsAt(viewer) })
  ).toString("base64url");
  return `${payload}.${mac(payload, key)}`;
}

/** The cookie's value for `viewer`, or null when BCS proposals are not configured. */
export function signSession(viewer: BcsViewer): string | null {
  return sign(viewer, "session");
}

/**
 * A `bep bcs` CLI token for `viewer`, which /bcs/token shows to a signed-in member. It ends when
 * the cookie it was made from does, so making one late in a session does not extend access.
 */
export function signCliToken(viewer: BcsViewer): string | null {
  const signed = sign(viewer, "cli");
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
    const now = Date.now() / 1000;
    if (claims.use !== use) return null;
    if (typeof claims.githubId !== "string" || typeof claims.login !== "string") return null;
    if (claims.via !== "org" || typeof claims.checkedAt !== "number") return null;
    const viewer: BcsViewer = {
      githubId: claims.githubId,
      login: claims.login,
      via: "org",
      checkedAt: claims.checkedAt,
    };
    // Expired, or signed to outlast its membership check.
    if (typeof claims.exp !== "number" || claims.exp < now) return null;
    if (accessEndsAt(viewer) < now || claims.exp > accessEndsAt(viewer)) return null;
    return viewer;
  } catch {
    return null;
  }
}

export const sessionCookie = {
  httpOnly: true,
  secure: process.env.NODE_ENV === "production",
  sameSite: "lax" as const,
  path: "/",
  maxAge: ACCESS_SECONDS,
};

/**
 * The signed-in team member viewing this request, or null. In development, BCS_DEV_VIEWER (a
 * GitHub login) stands in, since GitHub's login returns to the production site.
 */
export async function currentViewer(): Promise<BcsViewer | null> {
  if (process.env.NODE_ENV !== "production" && process.env.BCS_DEV_VIEWER) {
    return {
      githubId: "dev",
      login: process.env.BCS_DEV_VIEWER,
      via: "org",
      checkedAt: Math.floor(Date.now() / 1000),
    };
  }
  const jar = await cookies();
  return verifySession(jar.get(BCS_COOKIE)?.value);
}
