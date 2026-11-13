"use client";

// The BCS proposals board, as the BEP one: a kanban by status (the default) or a list filtered by
// status and sorted by update, both searchable by number, title, author or shepherd. The view and
// the filters are kept in the address, so a link shows the same board.
import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { formatDistanceToNow } from "date-fns";
import { ArrowUpDown, Columns3, FileText, LayoutList, MessageSquare, Search } from "lucide-react";
import { BepStatusBadge } from "@/components/bep/bep-status";
import { badgeVariants } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { BCS_STATUSES, type BcsStatus, bcsId } from "@/lib/bcs-convex";

export type BoardProposal = {
  number: number;
  title: string;
  status: BcsStatus;
  authors: string[];
  shepherds: string[];
  currentVersion: number;
  updatedAt: number;
  openComments: number;
  pageCount: number;
};

/** The same columns, names and colors as the BEP board. */
const COLUMNS: { status: BcsStatus; label: string; color: string }[] = [
  { status: "draft", label: "Draft", color: "bg-slate-500" },
  { status: "proposed", label: "Proposed", color: "bg-blue-500" },
  { status: "pending", label: "Pending", color: "bg-yellow-500" },
  { status: "accepted", label: "Accepted", color: "bg-green-500" },
  { status: "implemented", label: "Implemented", color: "bg-purple-500" },
  { status: "rejected", label: "Rejected", color: "bg-red-500" },
  { status: "superseded", label: "Superseded", color: "bg-orange-500" },
];

function matches(proposal: BoardProposal, query: string): boolean {
  if (!query) return true;
  const words = query.toLowerCase();
  return (
    proposal.title.toLowerCase().includes(words) ||
    bcsId(proposal.number).toLowerCase().includes(words) ||
    String(proposal.number) === words ||
    [...proposal.authors, ...proposal.shepherds].some((login) => login.toLowerCase().includes(words))
  );
}

function Meta({ proposal }: { proposal: BoardProposal }) {
  return (
    <div className="flex items-center gap-3 text-xs text-muted-foreground">
      <span className="flex items-center gap-1" title="Open comments">
        <MessageSquare className="h-3 w-3" />
        {proposal.openComments}
      </span>
      {proposal.pageCount > 1 && (
        <span className="flex items-center gap-1" title="Pages">
          <FileText className="h-3 w-3" />
          {proposal.pageCount}
        </span>
      )}
      <span>v{proposal.currentVersion}</span>
      <span className="ml-auto whitespace-nowrap">
        {formatDistanceToNow(proposal.updatedAt, { addSuffix: true })}
      </span>
    </div>
  );
}

function KanbanCard({ proposal }: { proposal: BoardProposal }) {
  return (
    <Link href={`/bcs/${proposal.number}`}>
      <Card className="hover:bg-accent/50 transition-colors">
        <CardContent className="p-3 space-y-2">
          <div>
            <span className="text-xs text-muted-foreground font-mono">{bcsId(proposal.number)}</span>
            <h4 className="text-sm font-medium leading-tight mt-0.5 line-clamp-3">{proposal.title}</h4>
          </div>
          <p className="text-xs text-muted-foreground truncate">
            {proposal.shepherds.length > 0
              ? `Shepherds: ${proposal.shepherds.join(", ")}`
              : proposal.authors.join(", ")}
          </p>
          <div className="pt-1 border-t">
            <Meta proposal={proposal} />
          </div>
        </CardContent>
      </Card>
    </Link>
  );
}

function Row({ proposal }: { proposal: BoardProposal }) {
  return (
    <Link href={`/bcs/${proposal.number}`}>
      <Card className="hover:bg-accent/50 transition-colors">
        <CardContent className="p-4 flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-4">
          <span className="font-mono text-sm text-muted-foreground sm:w-20">{bcsId(proposal.number)}</span>
          <div className="flex-1 min-w-0">
            <p className="font-medium truncate">{proposal.title}</p>
            <p className="text-xs text-muted-foreground truncate">
              {proposal.authors.join(", ")}
              {proposal.shepherds.length > 0 ? ` · Shepherds: ${proposal.shepherds.join(", ")}` : ""}
            </p>
          </div>
          <BepStatusBadge status={proposal.status} />
          <div className="sm:w-56">
            <Meta proposal={proposal} />
          </div>
        </CardContent>
      </Card>
    </Link>
  );
}

export function BcsBoard({ proposals }: { proposals: BoardProposal[] }) {
  const router = useRouter();
  const params = useSearchParams();
  const view = params.get("view") === "list" ? "list" : "kanban";
  const status = BCS_STATUSES.find((each) => each === params.get("status")) ?? "all";
  const oldestFirst = params.get("sort") === "oldest";
  const [query, setQuery] = useState(params.get("q") ?? "");

  /** Changes the board's address: one parameter, and the search box as typed. */
  const setParam = (key: string, value: string | null, search = query) => {
    const next = new URLSearchParams(params.toString());
    if (value === null) next.delete(key);
    else next.set(key, value);
    if (search) next.set("q", search);
    else next.delete("q");
    const address = next.toString();
    router.replace(`/bcs${address ? `?${address}` : ""}`, { scroll: false });
  };

  // The search goes into the address too, a moment after typing stops, so a link or a reload keeps it.
  const typed = useRef(false);
  useEffect(() => {
    if (!typed.current) return;
    const timer = setTimeout(() => setParam("q", query.trim() || null, query.trim()), 300);
    return () => clearTimeout(timer);
    // setParam reads the current address; only the typed search should schedule it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  const shown = useMemo(
    () =>
      proposals
        .filter((proposal) => matches(proposal, query.trim()))
        .filter((proposal) => view === "kanban" || status === "all" || proposal.status === status)
        .sort((a, b) => (oldestFirst ? a.updatedAt - b.updatedAt : b.updatedAt - a.updatedAt)),
    [proposals, query, view, status, oldestFirst]
  );

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row gap-4 items-start sm:items-center justify-between">
        {view === "list" ? (
          <div className="flex flex-wrap gap-2" role="group" aria-label="Filter by status">
            {(["all", ...BCS_STATUSES] as const).map((each) => (
              <button
                key={each}
                type="button"
                aria-pressed={status === each}
                onClick={() => setParam("status", each === "all" ? null : each)}
                className={`${badgeVariants({ variant: status === each ? "default" : "outline" })} cursor-pointer capitalize`}
              >
                {each === "all" ? "All" : each}
              </button>
            ))}
          </div>
        ) : (
          <span />
        )}
        <div className="flex w-full sm:w-auto items-center gap-2">
          <div className="relative flex-1 sm:flex-none sm:w-64">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search proposals…"
              aria-label="Search proposals"
              value={query}
              onChange={(event) => {
                typed.current = true;
                setQuery(event.target.value);
              }}
              className="pl-9"
            />
          </div>
          {view === "list" && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setParam("sort", oldestFirst ? null : "oldest")}
              className="shrink-0"
            >
              <ArrowUpDown className="h-4 w-4" />
              {oldestFirst ? "Oldest first" : "Newest first"}
            </Button>
          )}
          <div className="flex border rounded-md" role="group" aria-label="View">
            <Button
              type="button"
              variant={view === "list" ? "default" : "ghost"}
              size="sm"
              aria-label="List"
              aria-pressed={view === "list"}
              onClick={() => setParam("view", "list")}
              className="rounded-r-none"
            >
              <LayoutList className="h-4 w-4" />
            </Button>
            <Button
              type="button"
              variant={view === "kanban" ? "default" : "ghost"}
              size="sm"
              aria-label="Board"
              aria-pressed={view === "kanban"}
              onClick={() => setParam("view", null)}
              className="rounded-l-none"
            >
              <Columns3 className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </div>

      {view === "kanban" ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 xl:grid-cols-7 gap-4">
          {COLUMNS.map((column) => {
            const cards = shown.filter((proposal) => proposal.status === column.status);
            return (
              <section key={column.status} aria-label={column.label} className="flex flex-col min-h-[200px]">
                <div className="flex items-center gap-2 mb-3 pb-2 border-b">
                  <div className={`w-3 h-3 rounded-full ${column.color}`} />
                  <h3 className="font-medium text-sm">{column.label}</h3>
                  <span className="text-xs text-muted-foreground ml-auto">{cards.length}</span>
                </div>
                <div className="flex flex-col gap-2 flex-1">
                  {cards.length > 0 ? (
                    cards.map((proposal) => <KanbanCard key={proposal.number} proposal={proposal} />)
                  ) : (
                    <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground bg-muted/30 rounded-md">
                      None
                    </div>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      ) : shown.length > 0 ? (
        <div className="flex flex-col gap-2">
          {shown.map((proposal) => (
            <Row key={proposal.number} proposal={proposal} />
          ))}
        </div>
      ) : (
        <p className="text-center py-12 text-muted-foreground">
          {proposals.length === 0 ? "No proposals yet." : "No proposals match."}
        </p>
      )}
    </div>
  );
}
