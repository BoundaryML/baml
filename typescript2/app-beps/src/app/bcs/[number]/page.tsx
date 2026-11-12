import Link from "next/link";
import { notFound } from "next/navigation";
import { format } from "date-fns";
import { api } from "../../../../convex/_generated/api";
import { BepStatusBadge } from "@/components/bep/bep-status";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { currentViewer } from "@/lib/bcs-auth";
import { BCS_STATUSES, bcsConvex, bcsId } from "@/lib/bcs-convex";
import { saveVersion, updateProposal } from "../actions";
import { HtmlField } from "../proposal-fields";
import { TeamOnly } from "../team-only";

export const dynamic = "force-dynamic";

/**
 * The frame a proposal's page renders in. Without allow-same-origin its scripts run in an opaque
 * origin: they can draw the page's diagrams but not read this site, its cookies or its storage.
 */
const FRAME_SANDBOX = "allow-scripts allow-popups allow-popups-to-escape-sandbox";

export default async function BcsProposal({
  params,
  searchParams,
}: {
  params: Promise<{ number: string }>;
  searchParams: Promise<{ v?: string }>;
}) {
  const viewer = await currentViewer();
  if (!viewer) return <TeamOnly />;
  const number = Number((await params).number);
  const asked = Number((await searchParams).v);
  if (!Number.isInteger(number)) notFound();
  const { convex, secret } = bcsConvex();
  const proposal = await convex.query(api.bcs.get, {
    secret,
    number,
    version: Number.isInteger(asked) && asked > 0 ? asked : undefined,
  });
  if (!proposal) notFound();
  const old = proposal.version !== proposal.currentVersion;

  return (
    <div className="h-screen flex flex-col bg-background">
      <header className="border-b px-4 py-3 flex flex-wrap items-center gap-x-4 gap-y-2">
        <Link href="/bcs" className="text-sm text-muted-foreground hover:text-foreground">
          BCS proposals
        </Link>
        <span className="font-mono text-sm text-muted-foreground">{bcsId(proposal.number)}</span>
        <h1 className="text-lg font-semibold">{proposal.title}</h1>
        <BepStatusBadge status={proposal.status} />
        <span className="text-sm text-muted-foreground">
          {proposal.authors.join(", ")} · updated {format(proposal.updatedAt, "PP")}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <details className="relative">
            <summary className="cursor-pointer list-none text-sm rounded-md border px-3 py-1.5 hover:bg-muted">
              v{proposal.version}
              {old ? " (old)" : ""} · {proposal.versions.length} version
              {proposal.versions.length === 1 ? "" : "s"}
            </summary>
            <ul className="absolute right-0 z-10 mt-1 w-80 rounded-md border bg-background p-1 shadow-lg">
              {proposal.versions.map((version) => (
                <li key={version.version}>
                  <Link
                    href={`/bcs/${proposal.number}?v=${version.version}`}
                    className="block rounded px-2 py-1.5 text-sm hover:bg-muted"
                  >
                    <span className="font-medium">v{version.version}</span>{" "}
                    <span className="text-muted-foreground">
                      {version.createdBy} · {format(version.createdAt, "PP p")}
                    </span>
                    {version.note && (
                      <span className="block text-muted-foreground">{version.note}</span>
                    )}
                  </Link>
                </li>
              ))}
            </ul>
          </details>
          <a
            href={`/bcs/${proposal.number}/raw?v=${proposal.version}`}
            target="_blank"
            rel="noreferrer"
          >
            <Button variant="outline" size="sm">
              Open full page
            </Button>
          </a>
          <details className="relative">
            <summary className="cursor-pointer list-none text-sm rounded-md border px-3 py-1.5 hover:bg-muted">
              Edit
            </summary>
            <div className="absolute right-0 z-10 mt-1 w-[28rem] rounded-md border bg-background p-4 shadow-lg space-y-6">
              <form action={saveVersion.bind(null, proposal.number)} className="space-y-3">
                <h2 className="font-medium">Save a new version</h2>
                <HtmlField />
                <div className="space-y-2">
                  <Label htmlFor="note">What changed</Label>
                  <Input id="note" name="note" placeholder="Answered Paulo's questions" />
                </div>
                <Button type="submit" size="sm">
                  Save v{proposal.currentVersion + 1}
                </Button>
              </form>
              <form
                action={updateProposal.bind(null, proposal.number)}
                className="space-y-3 border-t pt-4"
              >
                <h2 className="font-medium">Title and status</h2>
                <Input name="title" defaultValue={proposal.title} aria-label="Title" />
                <select
                  name="status"
                  defaultValue={proposal.status}
                  aria-label="Status"
                  className="h-9 rounded-md border bg-background px-3 text-sm"
                >
                  {BCS_STATUSES.map((status) => (
                    <option key={status} value={status}>
                      {status}
                    </option>
                  ))}
                </select>
                <Button type="submit" size="sm" variant="outline">
                  Update
                </Button>
              </form>
            </div>
          </details>
        </div>
      </header>
      <iframe
        title={proposal.title}
        sandbox={FRAME_SANDBOX}
        srcDoc={proposal.html}
        className="flex-1 w-full border-0 bg-white"
      />
    </div>
  );
}
