import Link from "next/link";
import { notFound } from "next/navigation";
import { format } from "date-fns";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { recordDecision, removeDecision } from "../../actions";
import { TeamOnly } from "../../team-only";
import { BcsUnavailable } from "../../unavailable";
import { loadTracking } from "../tracking";
import { DecisionForm, RemoveButton } from "../tracking-forms";

export const dynamic = "force-dynamic";
export const metadata = { title: "BCS proposal decisions" };

/** A proposal's decisions, newest first, and the form to record one (from a thread with ?from=). */
export default async function BcsDecisions({
  params,
  searchParams,
}: {
  params: Promise<{ number: string }>;
  searchParams: Promise<{ from?: string }>;
}) {
  if (!(await currentViewer())) return <TeamOnly />;
  const number = Number((await params).number);
  if (!Number.isInteger(number)) notFound();
  const loaded = await loadTracking(number, (await searchParams).from);
  if ("failure" in loaded) return <BcsUnavailable message={loaded.failure ?? "BCS is unavailable."} />;
  if (!loaded.proposal || !loaded.tracking) notFound();
  const { proposal, tracking, threads, source } = loaded;
  const threadPage = new Map(threads.map((thread) => [thread.id, thread.page]));

  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-3xl mx-auto px-4 py-10 space-y-6">
        <div className="flex flex-wrap items-baseline gap-3">
          <Link href={`/bcs/${number}`} className="text-sm text-muted-foreground hover:text-foreground">
            {bcsId(number)} · {proposal.title}
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">Decisions</h1>
          <Link href={`/bcs/${number}/issues`} className="ml-auto text-sm underline text-muted-foreground hover:text-foreground">
            Open issues
          </Link>
        </div>
        <DecisionForm
          action={recordDecision.bind(null, number)}
          sourceId={source?.id}
          quote={source?.quote}
          title={source?.title}
        />
        {tracking.decisions.length === 0 ? (
          <p className="text-muted-foreground">No decisions recorded yet.</p>
        ) : (
          <ul className="space-y-3">
            {tracking.decisions.map((decision) => (
              <li key={decision.id} className="rounded-lg border p-4 space-y-2">
                <div className="flex items-baseline justify-between gap-3">
                  <h2 className="font-medium">{decision.title}</h2>
                  <RemoveButton action={removeDecision.bind(null, number)} name="decisionId" id={decision.id} what="decision" />
                </div>
                <p className="text-sm whitespace-pre-wrap">{decision.description}</p>
                {decision.rationale && (
                  <p className="text-sm text-muted-foreground whitespace-pre-wrap">Why: {decision.rationale}</p>
                )}
                <p className="text-xs text-muted-foreground">
                  Recorded by {decision.decidedBy} on {format(decision.decidedAt, "PP")}
                  {decision.participants.length > 1 && ` · with ${decision.participants.filter((p) => p !== decision.decidedBy).join(", ")}`}
                  {decision.sourceCommentIds.map((id) => (
                    <span key={id}>
                      {" · "}
                      <Link href={bcsPagePath(number, threadPage.get(id) ?? "", undefined, id)} className="underline">
                        the thread
                      </Link>
                    </span>
                  ))}
                </p>
              </li>
            ))}
          </ul>
        )}
      </main>
    </div>
  );
}
