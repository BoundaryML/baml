// What the decisions and issues pages share: loading a proposal's tracking, and the comment thread
// a ?from= link names, to start a decision or issue from it.
import { api } from "../../../../convex/_generated/api";
import { bcsCall } from "@/lib/bcs-convex";

export async function loadTracking(number: number, from?: string) {
  const [found, tracking, feed, everyone] = await Promise.all([
    bcsCall((convex, secret) => convex.query(api.bcs.get, { secret, number })),
    bcsCall((convex, secret) => convex.query(api.bcsTracking.list, { secret, number })),
    bcsCall((convex, secret) => convex.query(api.bcsComments.feed, { secret, number })),
    bcsCall((convex, secret) => convex.query(api.bcs.list, { secret })),
  ]);
  if (!found.ok) return { failure: found.failure.message } as const;
  if (!tracking.ok) return { failure: tracking.failure.message } as const;
  const threads = feed.ok ? feed.value ?? [] : [];
  const source = from ? threads.find((thread) => thread.id === from) : undefined;
  return {
    proposal: found.value,
    tracking: tracking.value,
    threads,
    source: source && {
      id: source.id,
      quote: source.deleted ? "a deleted comment" : source.body.slice(0, 300),
      // A starting title: the comment's first line.
      title: (source.deleted ? "" : source.body.split("\n")[0]).slice(0, 120),
    },
    people: everyone.ok ? everyone.value.people : [],
  } as const;
}
