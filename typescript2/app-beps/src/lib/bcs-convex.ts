// The server's client of the BCS Convex functions, with the secret they require. Server only:
// the secret never reaches the browser.
import { ConvexHttpClient } from "convex/browser";

export function bcsConvex(): { convex: ConvexHttpClient; secret: string } {
  const url = process.env.NEXT_PUBLIC_CONVEX_URL;
  const secret = process.env.BCS_CONVEX_SECRET;
  if (!url || !secret) {
    throw new Error("BCS proposals need NEXT_PUBLIC_CONVEX_URL and BCS_CONVEX_SECRET");
  }
  return { convex: new ConvexHttpClient(url), secret };
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
