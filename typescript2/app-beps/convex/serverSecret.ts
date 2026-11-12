// The secret only this app's Next.js server holds (BCS_CONVEX_SECRET, named for the BCS proposals
// that needed it first). Functions that take it can only be reached through the server, after it
// has checked who is asking, since the browser can call any public Convex function directly.
import { ConvexError } from "convex/values";

/** Refuses a call without the server secret, comparing in constant time. */
export function requireServerSecret(given: string) {
  // Trimmed on both sides: a secret pasted into a dashboard often gains a trailing newline.
  const expected = process.env.BCS_CONVEX_SECRET?.trim();
  if (!expected || expected.length < 32) {
    throw new ConvexError({
      kind: "not_configured",
      message: "BCS_CONVEX_SECRET is not set in Convex (32 characters or more).",
    });
  }
  const secret = given.trim();
  let difference = secret.length === expected.length ? 0 : 1;
  for (let i = 0; i < expected.length; i++) {
    difference |= expected.charCodeAt(i) ^ (i < secret.length ? secret.charCodeAt(i) : 0);
  }
  if (difference !== 0) {
    throw new ConvexError({
      kind: "not_configured",
      message: "BCS_CONVEX_SECRET differs between the site and Convex.",
    });
  }
}
