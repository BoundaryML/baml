import Link from "next/link";
import { Button } from "@/components/ui/button";
import { bcsSessionConfigured } from "@/lib/bcs-auth";
import { BcsUnavailable } from "./unavailable";

/** What someone who is not signed in as a team member sees instead of a BCS page. */
export function TeamOnly() {
  // Without the session secret no sign-in can succeed, so say that rather than loop on sign-in.
  if (!bcsSessionConfigured()) {
    return <BcsUnavailable message="BCS_SESSION_SECRET is not set on the site." />;
  }
  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-6">
      <div className="max-w-md text-center space-y-4">
        <h1 className="text-2xl font-bold">BCS design proposals</h1>
        <p className="text-muted-foreground">
          These are for the BoundaryML team. Sign in with the GitHub account you use at
          BoundaryML to read them.
        </p>
        <div className="flex justify-center gap-2">
          <Link href="/api/auth/github">
            <Button>Sign in with GitHub</Button>
          </Link>
          <Link href="/">
            <Button variant="outline">Back to BEPs</Button>
          </Link>
        </div>
      </div>
    </div>
  );
}
