import { notFound } from "next/navigation";
import { IssueDetail } from "@/components/issues/issue-detail";
import { ISSUES, findIssue } from "@/lib/mock-data";

export function generateStaticParams() {
  return ISSUES.map((i) => ({ id: i.id }));
}

export default async function IssuePage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const issue = findIssue(id);
  if (!issue) notFound();
  return <IssueDetail issue={issue} />;
}
