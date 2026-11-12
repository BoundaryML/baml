import Link from "next/link";
import { CopyToken } from "./copy-token";
import { currentViewer, signCliToken } from "@/lib/bcs-auth";
import { TeamOnly } from "../team-only";

export const dynamic = "force-dynamic";
export const metadata = { title: "BCS CLI token" };

/** A fresh `bep bcs` token for the signed-in member, valid for 90 days. */
export default async function BcsToken() {
  const viewer = await currentViewer();
  if (!viewer) return <TeamOnly />;
  const token = signCliToken(viewer);
  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-2xl mx-auto px-4 py-10 space-y-6">
        <div className="flex items-baseline gap-3">
          <Link href="/bcs" className="text-sm text-muted-foreground hover:text-foreground">
            BCS proposals
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">CLI token</h1>
        </div>
        <p className="text-muted-foreground">
          The <code>bep</code> script pulls and pushes BCS proposals with this token. It is yours
          ({viewer.login}), works for 14 days, and a new one is made each time you open this page.
          Keep it out of git and chat.
        </p>
        {token ? (
          <CopyToken token={token} />
        ) : (
          <p className="text-destructive">BCS proposals are not configured on this server.</p>
        )}
        <pre className="rounded-md bg-muted p-4 text-sm overflow-x-auto">
          {`export BCS_TOKEN=<the token above>
./bep bcs pull                     # BCS/BCS-001-.../index.html
./bep bcs new "Title" design.html  # a new proposal
./bep bcs push 1 -n "What changed" # a new version of BCS-001`}
        </pre>
      </main>
    </div>
  );
}
