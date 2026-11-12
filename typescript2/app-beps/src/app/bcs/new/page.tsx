import Link from "next/link";
import { currentViewer } from "@/lib/bcs-auth";
import { createProposal } from "../actions";
import { NewProposalForm } from "../forms";
import { TeamOnly } from "../team-only";

export const dynamic = "force-dynamic";
export const metadata = { title: "New BCS proposal" };

export default async function NewBcsProposal() {
  if (!(await currentViewer())) return <TeamOnly />;
  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-2xl mx-auto px-4 py-10 space-y-6">
        <div className="flex items-baseline gap-3">
          <Link href="/bcs" className="text-sm text-muted-foreground hover:text-foreground">
            BCS proposals
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">New proposal</h1>
        </div>
        <NewProposalForm action={createProposal} />
      </main>
    </div>
  );
}
