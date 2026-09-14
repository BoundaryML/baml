"use client";

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { ArrowUpDown, Columns3, LayoutList, Search } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { Difficulty, Issue, StatusState, Subsystem } from "@/lib/types";
import { progress, relativeTime, stageInfo } from "@/lib/pipeline";
import { DifficultyBadge, StatusBadge, SubsystemBadge } from "./issue-status";
import { PipelineStrip } from "./pipeline-strip";

type ViewMode = "list" | "board";

const STATUS_OPTIONS: { value: StatusState | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: "open", label: "Open" },
  { value: "in_progress", label: "In progress" },
  { value: "merged", label: "Merged" },
  { value: "shipped", label: "Shipped" },
  { value: "deferred", label: "Deferred" },
  { value: "rejected", label: "Rejected" },
];

const SUBSYSTEMS: Subsystem[] = ["Syntax", "Compiler", "Runtime", "StdLibrary", "Tooling", "Unknown"];
const DIFFICULTIES: Difficulty[] = ["Trivial", "Easy", "Medium", "Hard"];

const BOARD_COLUMNS: { state: StatusState; label: string; color: string }[] = [
  { state: "open", label: "Open", color: "bg-blue-500" },
  { state: "in_progress", label: "In progress", color: "bg-amber-500" },
  { state: "merged", label: "Merged", color: "bg-purple-500" },
  { state: "shipped", label: "Shipped", color: "bg-green-600" },
  { state: "deferred", label: "Deferred", color: "bg-slate-500" },
  { state: "rejected", label: "Rejected", color: "bg-red-500" },
];

/** The clock the relative times are measured against; ticks every minute, the
 * precision relativeTime shows. */
function useNow() {
  const [now, setNow] = useState(0);
  useEffect(() => {
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

function IssueRow({ issue, now }: { issue: Issue; now: number }) {
  const stages = stageInfo(issue);
  const current = stages.find((s) => s.state === "running") ?? stages.find((s) => s.state === "failed");
  return (
    <Link
      href={`/issues/${issue.id}`}
      className="grid grid-cols-[minmax(0,1fr)_auto] sm:grid-cols-[minmax(0,1fr)_180px_150px] items-center gap-4 px-4 py-3 border-b last:border-b-0 hover:bg-accent/50 transition-colors"
    >
      <div className="min-w-0">
        <div className="flex items-center gap-2 min-w-0">
          <span className="font-mono text-xs text-muted-foreground shrink-0">{issue.id}</span>
          <span className="font-medium truncate">{issue.title}</span>
        </div>
        <div className="mt-1.5 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <SubsystemBadge subsystem={issue.subsystem} />
          <DifficultyBadge difficulty={issue.difficulty} />
          {issue.dataset === "eval" && <EvalBadge />}
          <span>{issue.shepherd ? `@${issue.shepherd}` : "unassigned"}</span>
          <span>·</span>
          <span>v{issue.version}</span>
          {now > 0 && (
            <>
              <span>·</span>
              <span>{relativeTime(issue.updated_at, now)}</span>
            </>
          )}
        </div>
      </div>
      <div className="hidden sm:block">
        <PipelineStrip stages={stages} />
        <div className="mt-1 text-[11px] text-muted-foreground truncate">
          {current ? `${current.stage}: ${current.detail}` : `${Math.round(progress(issue) * 100)}% through the pipeline`}
        </div>
      </div>
      <div className="justify-self-end">
        <StatusBadge issue={issue} />
      </div>
    </Link>
  );
}

function BoardCard({ issue }: { issue: Issue }) {
  return (
    <Link
      href={`/issues/${issue.id}`}
      className="block rounded-md border bg-card p-3 hover:bg-accent/50 transition-colors"
    >
      <div className="flex items-center gap-2 font-mono text-[11px] text-muted-foreground">
        {issue.id}
        {issue.dataset === "eval" && <EvalBadge />}
      </div>
      <div className="mt-0.5 text-sm font-medium leading-snug line-clamp-2">{issue.title}</div>
      <div className="mt-2 flex items-center justify-between gap-2">
        <PipelineStrip stages={stageInfo(issue)} />
        <DifficultyBadge difficulty={issue.difficulty} />
      </div>
    </Link>
  );
}

export function IssueList({ issues }: { issues: Issue[] }) {
  const now = useNow();
  const [status, setStatus] = useState<StatusState | "all">("all");
  const [subsystem, setSubsystem] = useState<Subsystem | "all">("all");
  const [difficulty, setDifficulty] = useState<Difficulty | "all">("all");
  const [query, setQuery] = useState("");
  const [oldestFirst, setOldestFirst] = useState(false);
  const [view, setView] = useState<ViewMode>("list");
  const [showEval, setShowEval] = useState(false);
  const evalCount = useMemo(() => issues.filter((i) => i.dataset === "eval").length, [issues]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return issues
      .filter((i) => showEval || i.dataset !== "eval")
      .filter((i) => view === "board" || status === "all" || i.status.state === status)
      .filter((i) => subsystem === "all" || i.subsystem === subsystem)
      .filter((i) => difficulty === "all" || i.difficulty === difficulty)
      .filter(
        (i) =>
          !q ||
          i.title.toLowerCase().includes(q) ||
          i.id.toLowerCase().includes(q) ||
          (i.shepherd ?? "").toLowerCase().includes(q),
      )
      .sort((a, b) =>
        oldestFirst
          ? a.updated_at.localeCompare(b.updated_at)
          : b.updated_at.localeCompare(a.updated_at),
      );
  }, [issues, status, subsystem, difficulty, query, oldestFirst, view, showEval]);

  return (
    <div className="space-y-4">
      <div className="flex flex-col gap-3">
        <div className="flex flex-col lg:flex-row gap-3 lg:items-center justify-between">
          {view === "list" ? (
            <div className="flex flex-wrap gap-1.5">
              {STATUS_OPTIONS.map((o) => (
                <Badge
                  key={o.value}
                  variant={status === o.value ? "default" : "outline"}
                  className="cursor-pointer select-none"
                  onClick={() => setStatus(o.value)}
                >
                  {o.label}
                </Badge>
              ))}
            </div>
          ) : (
            <div />
          )}
          <div className="flex items-center gap-2">
            <div className="relative flex-1 lg:w-64">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder="Search issues, ids, shepherds"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                className="pl-9"
              />
            </div>
            <Button variant="outline" size="sm" onClick={() => setOldestFirst((v) => !v)} className="shrink-0">
              <ArrowUpDown className="h-4 w-4" />
              {oldestFirst ? "Oldest" : "Newest"}
            </Button>
            <div className="flex border rounded-md">
              <Button
                variant={view === "list" ? "default" : "ghost"}
                size="sm"
                onClick={() => setView("list")}
                className="rounded-r-none"
                aria-label="List view"
              >
                <LayoutList className="h-4 w-4" />
              </Button>
              <Button
                variant={view === "board" ? "default" : "ghost"}
                size="sm"
                onClick={() => setView("board")}
                className="rounded-l-none"
                aria-label="Board view"
              >
                <Columns3 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground">
          <label className="flex items-center gap-1.5" htmlFor="filter-subsystem">
            subsystem
            <select
              id="filter-subsystem"
              value={subsystem}
              onChange={(e) => setSubsystem(e.target.value as Subsystem | "all")}
              className="rounded border bg-background px-1.5 py-0.5 text-xs text-foreground"
            >
              <option value="all">all</option>
              {SUBSYSTEMS.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-1.5" htmlFor="filter-difficulty">
            difficulty
            <select
              id="filter-difficulty"
              value={difficulty}
              onChange={(e) => setDifficulty(e.target.value as Difficulty | "all")}
              className="rounded border bg-background px-1.5 py-0.5 text-xs text-foreground"
            >
              <option value="all">all</option>
              {DIFFICULTIES.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </label>
          {evalCount > 0 && (
            <label className="flex items-center gap-1.5 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={showEval}
                onChange={(e) => setShowEval(e.target.checked)}
                className="accent-foreground"
              />
              show {evalCount} eval {evalCount === 1 ? "row" : "rows"}
            </label>
          )}
          <span className="ml-auto flex items-center gap-3">
            <Legend color="bg-stage-done" label="done" />
            <Legend color="bg-stage-running" label="running" />
            <Legend color="bg-stage-failed" label="failed" />
            <Legend color="bg-stage-todo" label="pending" />
          </span>
        </div>
      </div>

      {view === "list" ? (
        filtered.length > 0 ? (
          <div className="rounded-lg border bg-card">
            <div className="hidden sm:grid grid-cols-[minmax(0,1fr)_180px_150px] gap-4 px-4 py-2 border-b text-[11px] uppercase tracking-wide text-muted-foreground">
              <span>Issue</span>
              <span>Pipeline</span>
              <span className="justify-self-end">Status</span>
            </div>
            {filtered.map((i) => (
              <IssueRow key={i.id} issue={i} now={now} />
            ))}
          </div>
        ) : (
          <div className="text-center py-12 text-muted-foreground">No issues match.</div>
        )
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-3 xl:grid-cols-6 gap-4">
          {BOARD_COLUMNS.map((col) => {
            const items = filtered.filter((i) => i.status.state === col.state);
            return (
              <div key={col.state} className="flex flex-col min-h-[160px]">
                <div className="flex items-center gap-2 mb-2 pb-2 border-b">
                  <div className={`w-2.5 h-2.5 rounded-full ${col.color}`} />
                  <h3 className="font-medium text-sm">{col.label}</h3>
                  <span className="text-xs text-muted-foreground ml-auto tabular-nums">{items.length}</span>
                </div>
                <div className="flex flex-col gap-2 flex-1">
                  {items.length > 0 ? (
                    items.map((i) => <BoardCard key={i.id} issue={i} />)
                  ) : (
                    <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground bg-muted/30 rounded-md">
                      none
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Rows the evals wrote (dataset = eval): never a real user's report. */
function EvalBadge() {
  return (
    <Badge variant="outline" className="border-dashed text-[10px] uppercase tracking-wide" title="Written by the evals, not a real report">
      eval
    </Badge>
  );
}

function Legend({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1">
      <span className={`inline-block h-1.5 w-4 rounded-sm ${color}`} />
      {label}
    </span>
  );
}
