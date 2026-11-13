import Link from "next/link";
import { notFound } from "next/navigation";
import { format } from "date-fns";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { openIssue, removeIssue, updateIssue } from "../../actions";
import { TeamOnly } from "../../team-only";
import { BcsUnavailable } from "../../unavailable";
import { loadTracking } from "../tracking";
import { IssueActions, IssueForm, RemoveButton } from "../tracking-forms";

export const dynamic = "force-dynamic";
export const metadata = { title: "BCS proposal issues" };

/** A proposal's open issues first, then resolved ones, and the form to open one (from a thread with ?from=). */
export default async function BcsIssues({
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
  const { proposal, tracking, threads, source, people } = loaded;
  const threadPage = new Map(threads.map((thread) => [thread.id, thread.page]));
  const open = tracking.issues.filter((issue) => !issue.resolved).length;

  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-3xl mx-auto px-4 py-10 space-y-6">
        <div className="flex flex-wrap items-baseline gap-3">
          <Link href={`/bcs/${number}`} className="text-sm text-muted-foreground hover:text-foreground">
            {bcsId(number)} · {proposal.title}
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">Open issues{open > 0 ? ` · ${open}` : ""}</h1>
          <Link href={`/bcs/${number}/decisions`} className="ml-auto text-sm underline text-muted-foreground hover:text-foreground">
            Decisions
          </Link>
        </div>
        <IssueForm
          action={openIssue.bind(null, number)}
          sourceId={source?.id}
          quote={source?.quote}
          title={source?.title}
          people={people}
        />
        {tracking.issues.length === 0 ? (
          <p className="text-muted-foreground">No issues yet.</p>
        ) : (
          <ul className="space-y-3">
            {tracking.issues.map((issue) => (
              <li key={issue.id} className={`rounded-lg border p-4 space-y-2 ${issue.resolved ? "opacity-75" : ""}`}>
                <div className="flex items-baseline justify-between gap-3">
                  <h2 className="font-medium">
                    {issue.resolved ? "✅ " : "🚩 "}
                    {issue.title}
                  </h2>
                  <RemoveButton action={removeIssue.bind(null, number)} name="issueId" id={issue.id} what="issue" />
                </div>
                {issue.description && <p className="text-sm whitespace-pre-wrap">{issue.description}</p>}
                <p className="text-xs text-muted-foreground">
                  Raised by {issue.raisedBy} on {format(issue.createdAt, "PP")}
                  {issue.assignedTo && ` · assigned to ${issue.assignedTo}`}
                  {issue.sourceCommentId && (
                    <>
                      {" · "}
                      <Link
                        href={bcsPagePath(number, threadPage.get(issue.sourceCommentId) ?? "", undefined, issue.sourceCommentId)}
                        className="underline"
                      >
                        the thread
                      </Link>
                    </>
                  )}
                </p>
                {issue.resolved && issue.resolution && (
                  <p className="text-sm border-l-2 border-green-500 pl-2 whitespace-pre-wrap">
                    Resolved by {issue.resolvedBy}
                    {issue.resolvedAt && ` on ${format(issue.resolvedAt, "PP")}`}: {issue.resolution}
                  </p>
                )}
                <IssueActions
                  action={updateIssue.bind(null, number)}
                  issueId={issue.id}
                  resolved={issue.resolved}
                  assignedTo={issue.assignedTo}
                />
              </li>
            ))}
          </ul>
        )}
      </main>
    </div>
  );
}
