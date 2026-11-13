import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "../../../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall, bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { parentChoices } from "@/lib/bcs-outline";
import { savePage } from "../../actions";
import { EditPageForm } from "../../forms";
import { TeamOnly } from "../../team-only";
import { BcsUnavailable } from "../../unavailable";

export const dynamic = "force-dynamic";
export const metadata = { title: "Edit BCS page" };

/** Edits one page of a proposal's current version, starting from what it holds now. */
export default async function EditBcsPage({
  params,
  searchParams,
}: {
  params: Promise<{ number: string }>;
  searchParams: Promise<{ page?: string }>;
}) {
  const viewer = await currentViewer();
  if (!viewer) return <TeamOnly />;
  const number = Number((await params).number);
  const slug = (await searchParams).page ?? "";
  if (!Number.isInteger(number)) notFound();
  const found = await bcsCall((convex, secret) =>
    convex.query(api.bcs.get, { secret, number, page: slug || undefined })
  );
  if (!found.ok) return <BcsUnavailable message={found.failure.message} />;
  const proposal = found.value;
  if (!proposal) notFound();
  const page = proposal.page;
  return (
    <div className="min-h-screen bg-background">
      <main className="max-w-4xl mx-auto px-4 py-10 space-y-6">
        <div className="flex flex-wrap items-baseline gap-3">
          <Link
            href={bcsPagePath(number, slug)}
            className="text-sm text-muted-foreground hover:text-foreground"
          >
            {bcsId(number)} · {proposal.title}
          </Link>
          <span className="text-muted-foreground">/</span>
          <h1 className="text-2xl font-bold">
            Edit {slug === "" ? "the main page" : `“${page.title}”`}
          </h1>
        </div>
        <EditPageForm
          action={savePage.bind(null, number, slug)}
          baseVersion={proposal.currentVersion}
          content={page.content}
          format={page.format}
          parentSlug={page.parentSlug}
          parents={slug === "" ? undefined : parentChoices(proposal.outline, slug)}
        />
      </main>
    </div>
  );
}
