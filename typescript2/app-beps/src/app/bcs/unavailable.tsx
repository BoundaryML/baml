import Link from "next/link";
import { Button } from "@/components/ui/button";

/**
 * What a team member sees when this deployment cannot reach the BCS tables: a secret is missing,
 * or the site's and Convex's copies differ. Only members see it, so it names the setting.
 */
export function BcsUnavailable({ message }: { message: string }) {
  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-6">
      <div className="max-w-md text-center space-y-4">
        <h1 className="text-2xl font-bold">BCS proposals are not set up here</h1>
        <p className="text-muted-foreground">{message}</p>
        <p className="text-sm text-muted-foreground">
          The BEPs README, under &ldquo;BCS design proposals&rdquo;, lists the settings for Vercel
          and Convex.
        </p>
        <Link href="/">
          <Button variant="outline">Back to BEPs</Button>
        </Link>
      </div>
    </div>
  );
}
