import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { format } from "date-fns";
import { api } from "../../../../convex/_generated/api";
import { BepStatusBadge } from "@/components/bep/bep-status";
import { buttonVariants } from "@/components/ui/button";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall, bcsId, bcsPagePath } from "@/lib/bcs-convex";
import { flattenOutline } from "@/lib/bcs-outline";
import { renderBcsPage } from "@/lib/bcs-render";
import {
  addComment,
  removePage,
  replyToComment,
  setCommentResolved,
  updateProposal,
} from "../actions";
import { TeamOnly } from "../team-only";
import { BcsUnavailable } from "../unavailable";
import { RemovePageForm, UpdateProposalForm } from "../forms";
import { BcsReader } from "./reader";

export const dynamic = "force-dynamic";

export default async function BcsProposal({
  params,
  searchParams,
}: {
  params: Promise<{ number: string }>;
  searchParams: Promise<{ v?: string; page?: string; comment?: string }>;
}) {
  const viewer = await currentViewer();
  if (!viewer) return <TeamOnly />;
  const number = Number((await params).number);
  const query = await searchParams;
  const asked = Number(query.v);
  const askedVersion = Number.isInteger(asked) && asked > 0 ? asked : undefined;
  if (!Number.isInteger(number)) notFound();
  // A ?comment= link opens its thread's page, whatever page the link named (or none).
  const linked = query.comment
    ? await bcsCall((convex, secret) =>
        convex.query(api.bcsComments.locate, { secret, number, commentId: query.comment! })
      )
    : undefined;
  const thread = linked?.ok ? linked.value : null;
  if (thread && thread.page !== (query.page ?? "")) {
    redirect(bcsPagePath(number, thread.page, askedVersion, thread.id));
  }
  const found = await bcsCall((convex, secret) =>
    convex.query(api.bcs.get, {
      secret,
      number,
      version: Number.isInteger(asked) && asked > 0 ? asked : undefined,
      page: query.page || undefined,
    })
  );
  if (!found.ok) return <BcsUnavailable message={found.failure.message} />;
  const proposal = found.value;
  // A sub-page the chosen version does not have (added later, or removed since): a linked thread's
  // own version has it; otherwise open the chosen version's main page.
  if (!proposal && query.page) {
    if (thread && thread.page === query.page && thread.version !== askedVersion) {
      redirect(bcsPagePath(number, thread.page, thread.version, thread.id));
    }
    redirect(bcsPagePath(number, "", askedVersion));
  }
  if (!proposal) notFound();
  const old = proposal.version !== proposal.currentVersion;
  // Links keep the version being read; the current one needs no ?v.
  const shownVersion = old ? proposal.version : undefined;
  const page = proposal.page;
  const outline = flattenOutline(proposal.outline);
  const comments = await bcsCall((convex, secret) =>
    convex.query(api.bcsComments.list, { secret, number, page: page.slug })
  );
  // Without its comments the proposal still reads; the panel says why they are missing.
  const { threads, openByPage } = (comments.ok && comments.value) || { threads: [], openByPage: {} };
  const rendered = await renderBcsPage({
    format: page.format,
    content: page.content,
    title: page.slug === "" ? proposal.title : `${page.title} · ${proposal.title}`,
    pageHref: (slug) => bcsPagePath(proposal.number, slug, shownVersion),
  });

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
            <ul className="absolute right-0 z-10 mt-1 w-80 max-h-[70vh] overflow-y-auto rounded-md border bg-background p-1 shadow-lg">
              {proposal.versions.map((version) => (
                <li key={version.version}>
                  <Link
                    href={bcsPagePath(proposal.number, page.slug, version.version)}
                    className="block rounded px-2 py-1.5 text-sm hover:bg-muted"
                  >
                    <span className="font-medium">v{version.version}</span>{" "}
                    <span className="text-muted-foreground">
                      {version.createdBy} · {format(version.createdAt, "PP p")}
                    </span>
                    {version.note && (
                      <span className="block text-muted-foreground">{version.note}</span>
                    )}
                    {version.changed && version.changed.length > 0 && (
                      <span className="block text-xs text-muted-foreground">
                        Changed: {version.changed.join(", ")}
                      </span>
                    )}
                  </Link>
                </li>
              ))}
            </ul>
          </details>
          <a
            href={`/bcs/${proposal.number}/raw?${new URLSearchParams({
              v: String(proposal.version),
              ...(page.slug ? { page: page.slug } : {}),
            })}`}
            target="_blank"
            rel="noreferrer"
            className={buttonVariants({ variant: "outline", size: "sm" })}
          >
            Open full page
          </a>
          {old ? (
            <Link
              href={bcsPagePath(proposal.number, page.slug)}
              className={buttonVariants({ variant: "outline", size: "sm" })}
            >
              Go to the current version
            </Link>
          ) : (
            <details className="relative">
              <summary className="cursor-pointer list-none text-sm rounded-md border px-3 py-1.5 hover:bg-muted">
                Edit
              </summary>
              <div className="absolute right-0 z-10 mt-1 w-80 rounded-md border bg-background p-4 shadow-lg space-y-4">
                <div className="flex flex-col gap-2">
                  <Link
                    href={`/bcs/${proposal.number}/edit${page.slug ? `?page=${page.slug}` : ""}`}
                    className={buttonVariants({ size: "sm" })}
                  >
                    Edit this page
                  </Link>
                  <Link
                    href={`/bcs/${proposal.number}/add${page.slug ? `?parent=${page.slug}` : ""}`}
                    className={buttonVariants({ variant: "outline", size: "sm" })}
                  >
                    Add a sub-page{page.slug ? " here" : ""}
                  </Link>
                </div>
                {page.slug !== "" && (
                  <RemovePageForm
                    action={removePage.bind(null, proposal.number, page.slug)}
                    baseVersion={proposal.currentVersion}
                    title={page.title}
                  />
                )}
                <div className="border-t pt-4">
                  <UpdateProposalForm
                    action={updateProposal.bind(null, proposal.number)}
                    title={proposal.title}
                    status={proposal.status}
                  />
                </div>
              </div>
            </details>
          )}
        </div>
      </header>
      <div className="flex flex-1 min-h-0">
        {outline.length > 1 && (
          <nav aria-label="Pages" className="w-64 shrink-0 overflow-y-auto border-r p-3 text-sm">
            <ul className="space-y-0.5">
              {outline.map((entry) => (
                <li key={entry.slug}>
                  <Link
                    href={bcsPagePath(proposal.number, entry.slug, shownVersion)}
                    aria-current={entry.slug === page.slug ? "page" : undefined}
                    style={{ paddingLeft: `${0.5 + entry.depth * 0.75}rem` }}
                    className={`block rounded py-1 pr-2 hover:bg-muted ${
                      entry.slug === page.slug ? "bg-muted font-medium" : "text-muted-foreground"
                    }`}
                  >
                    {entry.title}
                    {openByPage[entry.slug] ? (
                      <span
                        className="ml-1.5 rounded-full bg-violet-600/15 px-1.5 text-xs text-violet-700 dark:text-violet-300"
                        title={`${openByPage[entry.slug]} open comment${openByPage[entry.slug] === 1 ? "" : "s"}`}
                      >
                        {openByPage[entry.slug]}
                      </span>
                    ) : null}
                  </Link>
                </li>
              ))}
            </ul>
          </nav>
        )}
        <BcsReader
          // A new ?comment= remounts it, so the newly linked thread and block are focused.
          key={`${proposal.version}/${page.slug}/${query.comment ?? ""}`}
          title={page.title}
          document={rendered.document}
          blocks={rendered.blocks}
          number={proposal.number}
          version={proposal.version}
          threads={threads}
          // The thread a link to one of its replies points into.
          focusThread={thread?.id ?? query.comment}
          commentsError={comments.ok ? undefined : comments.failure.message}
          addComment={addComment.bind(null, proposal.number, page.slug, proposal.version)}
          reply={replyToComment.bind(null, proposal.number, proposal.version)}
          resolve={setCommentResolved.bind(null, proposal.number)}
        />
      </div>
    </div>
  );
}
