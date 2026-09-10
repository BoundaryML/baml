import { createHmac, timingSafeEqual } from "node:crypto";
import { cookies } from "next/headers";

export const SESSION_COOKIE = "atb2-session";
export function siteOrigin() {
  const url = new URL(process.env.ATB2_UI_URL ?? "");
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) throw new Error("Website URL is not configured");
  return url.origin;
}
function sessionKey() {
  const key = process.env.ATB2_UI_SESSION_SECRET;
  if (!key || key.length < 32) throw new Error("Website sign-in is not configured");
  return key;
}
export function signedSession(login: string) {
  const payload = Buffer.from(JSON.stringify({ login, expires: Date.now() + 60 * 60_000 })).toString("base64url");
  return payload + "." + createHmac("sha256", sessionKey()).update(payload).digest("base64url");
}
export function verifySession(value: string): string | null {
  try {
    const [payload, signature, extra] = value.split(".");
    if (!payload || !signature || extra || value.length > 1024) return null;
    const expected = createHmac("sha256", sessionKey()).update(payload).digest();
    const actual = Buffer.from(signature, "base64url");
    if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) return null;
    const data = JSON.parse(Buffer.from(payload, "base64url").toString());
    return typeof data.login === "string" && /^[a-zA-Z0-9-]{1,39}$/.test(data.login) && Number.isFinite(data.expires) && data.expires > Date.now() && data.expires <= Date.now() + 60 * 60_000 ? data.login : null;
  } catch { return null; }
}
export async function currentUser() {
  return verifySession((await cookies()).get(SESSION_COOKIE)?.value ?? "");
}
