import Link from "next/link";
import { notFound } from "next/navigation";
import { formatDistanceToNow } from "date-fns";
import { api } from "../../../../../convex/_generated/api";
import { badgeVariants } from "@/components/ui/badge";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall, bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { TeamOnly } from "../../team-only";
import { BcsUnavailable } from "../../unavailable";
import { KindBadge } from "../reader";

export const dynamic = "force-dynamic";
export const metadata = { title: "BCS proposal comments" };

const STATES = ["open", "resolved", "all"] as const;
const KINDS = ["all", "discussion", "concern", "question"] as const;
const KIND_LABELS = { discussion: "Discussion", concern: "Concern", question: "Question" } as const;

/** Every thread of a proposal across its pages, the most recently active first, as the BEP feed. */
export default async function BcsComments({
  params,
  searchParams,
}: {
  params: Promise<{ number: string }>;
  searchParams: Promise<{ state?: string; kind?: string }>;
}) {
  if (!(await currentViewer())) return <TeamOnly />;
  const number = Number((await params).number);
  if (!Number.isInteger(number)) notFound();
  const query = await searchParams;
  const state = STATES.find((each) => each === query.state) ?? "open";
  const kind = KINDS.find((each) => each === query.kind) ?? "all";
  const [found, feed] = await Promise.all([
    bcsCall((convex, secret) => convex.query(api.bcs.get, { secret, number })),
    bcsCall((convex, secret) => convex.query(api.bcsComments.feed, { secret, number })),
  ]);
  if (!found.ok) return <BcsUnavailable message={found.failure.message} />;
  if (!feed.ok) return <BcsUnavailable message={feed.failure.message} />;
  if (!found.value || !feed.value) notFound();
  const proposal = found.value;
  const shown = feed.value.filter(
    (thread) =>
      (state === "all" || (state === "open") === !thread.resolved) &&
      (kind === "all" || thread.type === kind)
  );
  const link = (next: { state?: string; kind?: string }) => {
    const search = new URLSearchParams();
    const s = next.state ?? state;
    const k = next.kind ?? kind;
    if (s !== "open") search.set("state", s);
    if (k !== "all") search.set("kind", k);
    const text = search.toString();
    return `/bcs/${number}/comments${text ? `?${text}` : ""}`;
  };

  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-3xl mx-auto px-4 py-10 space-y-6">
        <div className="flex flex-wrap items-baseline gap-3">
          <Link href={`/bcs/${number}`} className="text-sm text-muted-foreground hover:text-foreground">
            {bcsId(number)} · {proposal.title}
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">All comments</h1>
        </div>
        <div className="flex flex-wrap gap-2" aria-label="Filter comments">
          {STATES.map((each) => (
            <Link
              key={each}
              href={link({ state: each })}
              aria-current={state === each ? "page" : undefined}
              className={`${badgeVariants({ variant: state === each ? "default" : "outline" })} capitalize`}
            >
              {each}
            </Link>
          ))}
          <span className="mx-1 border-l" />
          {KINDS.map((each) => (
            <Link
              key={each}
              href={link({ kind: each })}
              aria-current={kind === each ? "page" : undefined}
              className={badgeVariants({ variant: kind === each ? "default" : "outline" })}
            >
              {each === "all" ? "All kinds" : KIND_LABELS[each]}
            </Link>
          ))}
        </div>
        {shown.length === 0 ? (
          <p className="text-muted-foreground">No comments here.</p>
        ) : (
          <ul className="space-y-3">
            {shown.map((thread) => (
              <li key={thread.id}>
                <Link
                  href={bcsPagePath(number, thread.page, undefined, thread.id)}
                  className="block rounded-lg border p-3 hover:bg-muted/50 space-y-1.5"
                >
                  <div className="flex flex-wrap items-baseline gap-x-2 text-xs text-muted-foreground">
                    <KindBadge type={thread.type} />
                    <span className="font-medium text-foreground">{thread.author}</span>
                    <span>on {thread.pageTitle}</span>
                    <span>{formatDistanceToNow(thread.createdAt, { addSuffix: true })}</span>
                    {thread.resolved && <span>· resolved{thread.resolvedBy ? ` by ${thread.resolvedBy}` : ""}</span>}
                  </div>
                  {thread.anchor && (
                    <p className="border-l-2 border-violet-400 pl-2 text-xs text-muted-foreground line-clamp-2">
                      {thread.anchor.text || "A diagram"}
                    </p>
                  )}
                  <p className="text-sm line-clamp-3 whitespace-pre-wrap">
                    {thread.deleted ? <em className="text-muted-foreground">This comment was deleted.</em> : thread.body}
                  </p>
                  {thread.replyCount > 0 && (
                    <p className="text-xs text-muted-foreground">
                      {thread.replyCount} repl{thread.replyCount === 1 ? "y" : "ies"}
                      {thread.lastReply &&
                        ` · last by ${thread.lastReply.author} ${formatDistanceToNow(thread.lastReply.createdAt, {
                          addSuffix: true,
                        })}`}
                    </p>
                  )}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </main>
    </div>
  );
}
