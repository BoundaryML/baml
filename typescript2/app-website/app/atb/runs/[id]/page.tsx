"use client";

// Run detail: the task prompt, the verdict, metrics, the agent's report,
// anchored findings, the files it wrote, and the full transcript.

import Link from "next/link";
import { use, useRef, useState } from "react";
import { motion } from "framer-motion";
import { useAtbState, useDoc, bamlRefLabel } from "@/app/atb/_lib/api";
import type { Task, Trophy } from "@/app/atb/_lib/types";
import { compact, duration, usd, wallClock } from "@/app/atb/_lib/format";
import {
  Card,
  EASE,
  KindChip,
  OutcomeBadge,
  Reveal,
  SectionHeader,
  Skeleton,
} from "@/app/atb/_components/ui";
import { Markdown } from "@/app/atb/_components/markdown";
import { TranscriptViewer } from "@/app/atb/_components/transcript";
import { CommentThread } from "@/app/atb/_components/comments";
import { useComments } from "@/app/atb/_lib/comments";
import { FilesIde } from "@/app/atb/_components/files-ide";

export default function RunPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  // The dispatch-time Slack link is /runs/<taskId> (no trophy exists yet). db.get(id)
  // returns whatever document carries that id, so distinguish a finished trophy from
  // an in-flight task by shape and show the mid-run view instead of choking on a
  // task's missing metrics.
  const doc = useDoc<Trophy & Task>("trophies", id);
  const trophy =
    doc && (doc as Trophy).outcome !== undefined ? (doc as unknown as Trophy) : null;
  const inflightTask =
    doc && (doc as Trophy).outcome === undefined && (doc as Task).prompt !== undefined
      ? (doc as unknown as Task)
      : null;
  const task = useDoc<Task>("tasks", trophy?.taskId ?? null);
  const comments = useComments(trophy?._id ?? null);
  const state = useAtbState();
  // Highlighting in the raw terminal opens the run-level composer with the quote.
  const [runQuote, setRunQuote] = useState<{ text: string; nonce: number } | null>(
    null,
  );
  const commentsRef = useRef<HTMLDivElement>(null);

  if (doc === undefined) return <RunSkeleton />;
  if (inflightTask)
    return (
      <InFlightRun
        task={inflightTask}
        trophyId={state?.trophies?.find((t) => t.taskId === id)?._id ?? null}
      />
    );
  if (trophy === null)
    return (
      <p className="pt-24 text-center text-atb-ink-3">run not found</p>
    );

  const m = trophy.metrics ?? {};
  // bamlVersion is now a concrete toolchain version (e.g. "0.12.2-nightly.2026…")
  // for v2 runs; legacy runs stored a build sha matched against bamlBuilds. Show the
  // build ref if it matches, else the full version string, else (a raw sha) abbreviate.
  const looksLikeVersion = (v: string) =>
    v.includes(".") || v.includes("nightly") || v.includes("canary");
  const bamlLabel =
    trophy.bamlVersion === "coldstart"
      ? "cold start"
      : bamlRefLabel(
          (state?.builds ?? []).find((b) => b.sha === trophy.bamlVersion)?.ref,
        ) ??
        (trophy.bamlVersion
          ? looksLikeVersion(trophy.bamlVersion)
            ? trophy.bamlVersion
            : trophy.bamlVersion.slice(0, 8)
          : null);

  const files = Object.entries(trophy.filesCreated ?? {});

  // Issues this run produced - any issue whose evidence cites this trophy. Used to
  // deep-link the run to its tracked issues, and each finding to its issue by call.
  const runIssues = (state?.issues ?? []).filter((i) =>
    i.evidence?.some((e) => e.trophyId === trophy._id),
  );
  const issueByCall = new Map(
    runIssues.flatMap((i) =>
      (i.evidence ?? [])
        .filter((e) => e.trophyId === trophy._id && e.call_index != null)
        .map((e) => [e.call_index as number, i] as const),
    ),
  );
  const soleRunIssue = runIssues.length === 1 ? runIssues[0] : null;

  return (
    <div className="pt-12">
      {/* ---- header ---- */}
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <div className="flex items-center gap-2 text-xs text-atb-ink-3 mb-3">
          <Link href="/atb/runs" className="hover:text-atb-ink transition-colors">
            Runs
          </Link>
          <span>/</span>
          <span className="font-atb-mono">{trophy._id.slice(0, 12)}…</span>
          {trophy.isCohortReport && trophy.cohortId && (
            <Link
              href={`/atb/arena/${trophy.cohortId}`}
              className="ml-2 text-atb-accent-deep hover:text-atb-accent"
            >
              arena report →
            </Link>
          )}
        </div>
        <h1 className="font-atb-serif text-2xl sm:text-[1.7rem] font-semibold tracking-tight leading-snug max-w-3xl">
          {task?.prompt ?? (trophy.isCohortReport ? "Skill arena comparison report" : "…")}
        </h1>
        <div className="flex flex-wrap items-center gap-2.5 mt-4 text-xs">
          <OutcomeBadge outcome={trophy.outcome} />
          {task?.source && (
            <span className="text-atb-ink-3">
              via <span className="text-atb-ink-2">{task.source}</span>
            </span>
          )}
          {task?.skillRef && (
            <span className="font-atb-mono bg-atb-ivory border border-atb-line rounded-full px-2.5 py-0.5 text-atb-ink-2">
              skill: {task.skillRef}
            </span>
          )}
          {bamlLabel && (
            <span className="font-atb-mono bg-atb-ivory border border-atb-line rounded-full px-2.5 py-0.5 text-atb-ink-2">
              baml {bamlLabel}
            </span>
          )}
          <span className="text-atb-ink-3">{wallClock(trophy.createdAt)}</span>
        </div>
      </motion.div>

      {/* ---- metrics band ---- */}
      <Reveal className="mt-8">
        <div className="grid grid-cols-3 sm:grid-cols-4 lg:grid-cols-8 border border-atb-line rounded-2xl overflow-hidden bg-atb-ivory/50 divide-x divide-y sm:divide-y-0 divide-atb-line/60">
          <Metric label="turns" value={m.turns} />
          <Metric label="api calls" value={m.api_calls} />
          <Metric label="tool calls" value={m.tool_calls} />
          <Metric label="output tok" value={m.output_tokens} fmt={compact} />
          <Metric
            label="cache read"
            value={m.cache_read_tokens}
            fmt={compact}
          />
          <Metric label="cost" value={m.estimated_cost_usd} fmt={usd} />
          <Metric label="wall clock" value={m.wall_clock_ms} fmt={duration} />
          <Metric label="loc changed" value={m.loc_changed} />
        </div>
      </Reveal>

      {/* ---- narrative ---- */}
      {trophy.summary && (
        <Reveal className="mt-10">
          <SectionHeader title="Summary" hint="written by the agent" />
          <Card className="px-6 py-5">
            <p className="font-atb-serif text-[1.05rem] leading-relaxed text-atb-ink-2">
              {trophy.summary}
            </p>
          </Card>
        </Reveal>
      )}

      {((trophy.whatWentWell?.length ?? 0) > 0 ||
        (trophy.whatFailed?.length ?? 0) > 0) && (
        <Reveal className="mt-10 grid md:grid-cols-2 gap-4">
          {(trophy.whatWentWell?.length ?? 0) > 0 && (
            <Card className="px-6 py-5 border-atb-olive/25">
              <p className="text-[11px] uppercase tracking-wider text-atb-olive font-semibold mb-3">
                What went well
              </p>
              <ul className="space-y-2.5">
                {trophy.whatWentWell!.map((w, i) => (
                  <li key={i} className="text-sm text-atb-ink-2 leading-relaxed flex gap-2.5">
                    <span className="text-atb-olive mt-0.5 shrink-0">✓</span>
                    {w}
                  </li>
                ))}
              </ul>
            </Card>
          )}
          {(trophy.whatFailed?.length ?? 0) > 0 && (
            <Card className="px-6 py-5 border-atb-rust/25">
              <p className="text-[11px] uppercase tracking-wider text-atb-rust font-semibold mb-3">
                What failed
              </p>
              <ul className="space-y-2.5">
                {trophy.whatFailed!.map((w, i) => (
                  <li key={i} className="text-sm text-atb-ink-2 leading-relaxed flex gap-2.5">
                    <span className="text-atb-rust mt-0.5 shrink-0">✕</span>
                    {w}
                  </li>
                ))}
              </ul>
            </Card>
          )}
        </Reveal>
      )}

      {/* ---- findings ---- */}
      {(trophy.findings?.length ?? 0) > 0 && (
        <Reveal className="mt-10">
          <SectionHeader
            title={`Findings (${trophy.findings!.length})`}
            hint="these feed issue dedup"
          />
          <div className="space-y-3">
            {trophy.findings!.map((f, i) => {
              const fIssue =
                (f.anchor?.call_index != null
                  ? issueByCall.get(f.anchor.call_index)
                  : undefined) ?? soleRunIssue;
              return (
                <Card key={i} className="px-6 py-5">
                  <div className="flex items-start gap-3">
                    <KindChip kind={f.kind} />
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-atb-ink leading-snug">
                        {f.title}
                      </p>
                      <div className="mt-2">
                        <Markdown>{f.description}</Markdown>
                      </div>
                      {f.suggestion && (
                        <div className="mt-3 border-l-2 border-atb-accent pl-4 text-sm text-atb-ink-2 leading-relaxed">
                          <span className="text-atb-accent-deep font-medium">
                            suggested fix:{" "}
                          </span>
                          {f.suggestion}
                        </div>
                      )}
                      <div className="mt-3 flex items-center gap-4">
                        {fIssue && (
                          <Link
                            href={`/atb/issues/${fIssue._id}`}
                            className="text-xs font-atb-mono text-atb-accent-deep hover:text-atb-accent transition-colors"
                          >
                            tracked as issue →
                          </Link>
                        )}
                        {f.anchor?.turn_index != null && (
                          <a
                            href={`#turn-${f.anchor.turn_index}`}
                            className="text-xs font-atb-mono text-atb-accent-deep hover:text-atb-accent transition-colors"
                          >
                            ↓ jump to turn {f.anchor.turn_index}
                          </a>
                        )}
                      </div>
                    </div>
                  </div>
                </Card>
              );
            })}
          </div>
        </Reveal>
      )}

      {/* ---- issues this run produced ---- */}
      {runIssues.length > 0 && (
        <Reveal className="mt-10">
          <SectionHeader
            title={`Issues from this run (${runIssues.length})`}
            hint="deduped findings tracked on the board"
          />
          <div className="space-y-2">
            {runIssues.map((iss) => (
              <Link
                key={iss._id}
                href={`/atb/issues/${iss._id}`}
                className="block"
              >
                <Card className="px-5 py-3.5 hover:border-atb-accent/40 transition-colors">
                  <div className="flex items-center gap-3">
                    <KindChip kind={iss.kind} />
                    <span className="flex-1 min-w-0 text-sm text-atb-ink leading-snug">
                      {iss.title}
                    </span>
                    <span className="font-atb-mono text-[11px] text-atb-ink-3 shrink-0">
                      {iss.status}
                    </span>
                  </div>
                </Card>
              </Link>
            ))}
          </div>
        </Reveal>
      )}

      {/* ---- files the agent wrote ---- */}
      {files.length > 0 && (
        <Reveal className="mt-10">
          <SectionHeader title="Files written" />
          <FilesIde files={files} />
        </Reveal>
      )}

      {/* ---- full report ---- */}
      {trophy.reportMd && (
        <Reveal className="mt-10">
          <SectionHeader title="Report" />
          <Card className="px-6 py-2">
            <Markdown>{trophy.reportMd}</Markdown>
          </Card>
        </Reveal>
      )}

      {/* ---- transcript ---- */}
      {(trophy.turnLog?.length ?? 0) > 0 && (
        <Reveal className="mt-12">
          <SectionHeader title="Transcript" />
          <TranscriptViewer
            turnLog={trophy.turnLog!}
            transcriptStorageId={trophy.transcriptStorageId}
            trophyId={trophy._id}
            taskId={trophy.taskId}
            comments={comments}
            onQuote={(t) => {
              setRunQuote({ text: t, nonce: Date.now() });
              commentsRef.current?.scrollIntoView({
                behavior: "smooth",
                block: "start",
              });
            }}
          />
        </Reveal>
      )}

      {/* ---- run-level comments (turn-anchored ones live inline above) ---- */}
      <div ref={commentsRef}>
        <Reveal className="mt-12">
          <SectionHeader
            title="Comments"
            hint="feed the dedup agent: actionable ones become tickets"
          />
          <CommentThread
            trophyId={trophy._id}
            taskId={trophy.taskId}
            comments={(comments ?? []).filter((c) => c.turnIndex == null)}
            quoteRequest={runQuote}
          />
        </Reveal>
      </div>
    </div>
  );
}

function Metric({
  label,
  value,
  fmt,
}: {
  label: string;
  value?: number | null;
  fmt?: (v?: number | null) => string;
}) {
  return (
    <div className="px-4 py-3">
      <p className="font-atb-serif text-lg font-semibold tabular-nums">
        {value == null ? "—" : fmt ? fmt(value) : value.toLocaleString()}
      </p>
      <p className="text-[10px] uppercase tracking-wider text-atb-ink-3 mt-0.5">
        {label}
      </p>
    </div>
  );
}

function RunSkeleton() {
  return (
    <div className="pt-12 space-y-6">
      <Skeleton className="h-5 w-40" />
      <Skeleton className="h-16 w-3/4" />
      <Skeleton className="h-20" />
      <Skeleton className="h-40" />
    </div>
  );
}

// Mid-run view: shown when the URL id is a still-running task (the dispatch-time
// Slack link) that has not produced a trophy yet. Flips to the finished run as
// soon as a trophy for this task appears in state.
function InFlightRun({
  task,
  trophyId,
}: {
  task: Task;
  trophyId: string | null;
}) {
  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <div className="flex items-center gap-2 text-xs text-atb-ink-3 mb-3">
          <Link href="/atb/runs" className="hover:text-atb-ink transition-colors">
            Runs
          </Link>
          <span>/</span>
          <span className="font-atb-mono">{task._id.slice(0, 12)}…</span>
        </div>
        <h1 className="font-atb-serif text-2xl sm:text-[1.7rem] font-semibold tracking-tight leading-snug max-w-3xl">
          {task.prompt}
        </h1>
        <div className="flex flex-wrap items-center gap-2.5 mt-4 text-xs">
          <span className="inline-flex items-center gap-2 rounded-full border border-atb-line bg-atb-ivory px-3 py-1 text-atb-ink-2">
            <span className="h-2 w-2 rounded-full bg-atb-accent animate-pulse" />
            {task.status || "running"}
          </span>
          {task.source && (
            <span className="text-atb-ink-3">
              via <span className="text-atb-ink-2">{task.source}</span>
            </span>
          )}
          <span className="text-atb-ink-3">{wallClock(task.createdAt)}</span>
        </div>
      </motion.div>

      <Reveal className="mt-10">
        <Card className="px-6 py-8 text-center">
          {trophyId ? (
            <>
              <p className="text-atb-ink-2 mb-4">This run has finished.</p>
              <Link
                href={`/atb/runs/${trophyId}`}
                className="text-atb-accent-deep hover:text-atb-accent font-medium"
              >
                view the completed run →
              </Link>
            </>
          ) : (
            <p className="text-atb-ink-3 leading-relaxed max-w-md mx-auto">
              The agent is working on this task. The report, metrics, findings,
              and full transcript will appear here once the run completes.
            </p>
          )}
        </Card>
      </Reveal>
    </div>
  );
}
