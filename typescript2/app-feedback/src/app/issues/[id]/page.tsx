import { notFound } from "next/navigation";
import { IssueDetail } from "@/components/issues/issue-detail";
import { loadIssue } from "@/lib/db";

// On demand, never prerendered at build: the data source is decided by the
// server's environment, and the fetch cache (REVALIDATE_S) bounds the reads.
export const dynamic = "force-dynamic";

export default async function IssuePage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const issue = await loadIssue(id);
  if (!issue) notFound();
  return <IssueDetail issue={issue} />;
}
