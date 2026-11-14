import Link from "next/link";
import { Suspense } from "react";
import { api } from "../../../convex/_generated/api";
import { Button } from "@/components/ui/button";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall } from "@/lib/bcs-convex";
import { BcsBoard } from "./board";
import { TeamOnly } from "./team-only";
import { BcsUnavailable } from "./unavailable";

export const dynamic = "force-dynamic";
export const metadata = { title: "BCS design proposals" };

export default async function BcsProposals() {
  const viewer = await currentViewer();
  if (!viewer) return <TeamOnly />;
  const listed = await bcsCall((convex, secret) => convex.query(api.bcs.list, { secret }));
  if (!listed.ok) return <BcsUnavailable message={listed.failure.message} />;
  const { proposals } = listed.value;

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-[1600px] mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-baseline gap-3">
            <Link href="/" className="text-sm text-muted-foreground hover:text-foreground">
              BEPs
            </Link>
            <span className="text-muted-foreground">/</span>
            <h1 className="text-2xl font-bold">BCS design proposals</h1>
          </div>
          <span className="text-sm text-muted-foreground">
            Team only · {viewer.login} ·{" "}
            <Link href="/bcs/token" className="underline hover:text-foreground">
              CLI token
            </Link>
          </span>
        </div>
      </header>
      <main className="max-w-[1600px] mx-auto px-4 py-8 space-y-6">
        <div className="flex items-center justify-between">
          <p className="text-muted-foreground">
            System design proposals for Boundary Cloud, each a page with its diagrams.
          </p>
          <Link href="/bcs/new">
            <Button>New proposal</Button>
          </Link>
        </div>
        <Suspense>
          <BcsBoard proposals={proposals} />
        </Suspense>
      </main>
    </div>
  );
}
