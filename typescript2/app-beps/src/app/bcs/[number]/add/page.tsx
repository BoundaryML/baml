import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "../../../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall, bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { parentChoices } from "@/lib/bcs-outline";
import { addPage } from "../../actions";
import { AddPageForm } from "../../forms";
import { TeamOnly } from "../../team-only";
import { BcsUnavailable } from "../../unavailable";

export const dynamic = "force-dynamic";
export const metadata = { title: "Add a BCS page" };

/** Adds a sub-page to a proposal, under `?parent=` when given. */
export default async function AddBcsPage({
  params,
  searchParams,
}: {
  params: Promise<{ number: string }>;
  searchParams: Promise<{ parent?: string }>;
}) {
  const viewer = await currentViewer();
  if (!viewer) return <TeamOnly />;
  const number = Number((await params).number);
  const parent = (await searchParams).parent;
  if (!Number.isInteger(number)) notFound();
  const found = await bcsCall((convex, secret) => convex.query(api.bcs.get, { secret, number }));
  if (!found.ok) return <BcsUnavailable message={found.failure.message} />;
  const proposal = found.value;
  if (!proposal) notFound();
  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-4xl mx-auto px-4 py-10 space-y-6">
        <div className="flex flex-wrap items-baseline gap-3">
          <Link
            href={bcsPagePath(number, parent ?? "")}
            className="text-sm text-muted-foreground hover:text-foreground"
          >
            {bcsId(number)} · {proposal.title}
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">Add a sub-page</h1>
        </div>
        <AddPageForm
          action={addPage.bind(null, number)}
          baseVersion={proposal.currentVersion}
          parents={parentChoices(proposal.outline)}
          parentSlug={proposal.outline.some((page) => page.slug === parent) ? parent : undefined}
        />
      </main>
    </div>
  );
}
