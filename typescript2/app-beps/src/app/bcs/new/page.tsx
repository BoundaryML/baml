import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { currentViewer } from "@/lib/bcs-auth";
import { BCS_STATUSES } from "@/lib/bcs-convex";
import { createProposal } from "../actions";
import { HtmlField } from "../proposal-fields";
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
        <form action={createProposal} className="space-y-5">
          <div className="space-y-2">
            <Label htmlFor="title">Title</Label>
            <Input id="title" name="title" required placeholder="Alerts on Alien Commands" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="status">Status</Label>
            <select
              id="status"
              name="status"
              defaultValue="proposed"
              className="h-9 rounded-md border bg-background px-3 text-sm"
            >
              {BCS_STATUSES.map((status) => (
                <option key={status} value={status}>
                  {status}
                </option>
              ))}
            </select>
          </div>
          <HtmlField />
          <Button type="submit">Create</Button>
        </form>
      </main>
    </div>
  );
}
