// The server's calls to the BCS Convex functions, with the secret they require. Server only:
// the secret never reaches the browser.
import { ConvexHttpClient } from "convex/browser";
import { ConvexError } from "convex/values";
import type { BcsFailure } from "../../convex/bcs";

export type { BcsFailure };

export type BcsResult<T> = { ok: true; value: T } | { ok: false; failure: BcsFailure };

/**
 * Runs `call` against Convex with the server secret. A refusal the BCS functions explain (not set
 * up, stale save, no such proposal, bad input) comes back as a failure to show; anything else is
 * thrown, as a bug.
 */
export async function bcsCall<T>(
  call: (convex: ConvexHttpClient, secret: string) => Promise<T>
): Promise<BcsResult<T>> {
  const url = process.env.NEXT_PUBLIC_CONVEX_URL;
  // Trimmed, as Convex trims its copy: a pasted secret often gains a trailing newline.
  const secret = process.env.BCS_CONVEX_SECRET?.trim();
  if (!url || !secret) {
    return {
      ok: false,
      failure: { kind: "not_configured", message: "BCS_CONVEX_SECRET is not set on the site." },
    };
  }
  try {
    return { ok: true, value: await call(new ConvexHttpClient(url), secret) };
  } catch (error) {
    const failure = bcsFailure(error);
    if (failure) return { ok: false, failure };
    throw error;
  }
}

function bcsFailure(error: unknown): BcsFailure | null {
  if (!(error instanceof ConvexError)) return null;
  const data = error.data as Partial<BcsFailure> | undefined;
  return data && typeof data.kind === "string" && typeof data.message === "string"
    ? (data as BcsFailure)
    : null;
}

export const BCS_STATUSES = [
  "draft",
  "proposed",
  "pending",
  "accepted",
  "implemented",
  "rejected",
  "superseded",
] as const;
export type BcsStatus = (typeof BCS_STATUSES)[number];

export function bcsId(number: number): string {
  return `BCS-${String(number).padStart(3, "0")}`;
}
