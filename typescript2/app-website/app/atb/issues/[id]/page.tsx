"use client";

// Issue detail: full description, the fix suggestion, a minimal repro, and
// links back to every run that hit it. The full doc comes over the
// websocket; evidence labels come from the slim snapshot.

import Link from "next/link";
import { use } from "react";
import { motion } from "framer-motion";
import { useAtbState, useDoc, issueStatusLabel, skillRepoLabel } from "@/app/atb/_lib/api";
import type { Issue } from "@/app/atb/_lib/types";
import { timeAgo, usd, wallClock } from "@/app/atb/_lib/format";
import {
  Card,
  EASE,
  KindChip,
  Reveal,
  SectionHeader,
  Skeleton,
  StatusPill,
} from "@/app/atb/_components/ui";
import { Markdown } from "@/app/atb/_components/markdown";
import { CodeView } from "@/app/atb/_components/code-view";
import { useNow } from "@/app/atb/_components/use-now";

export default function IssuePage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const now = useNow();
  const issue = useDoc<Issue>("issues", id);
  const state = useAtbState();

  if (issue === undefined)
    return (
      <div className="pt-12 space-y-6">
        <Skeleton className="h-5 w-40" />
        <Skeleton className="h-14 w-3/4" />
        <Skeleton className="h-48" />
      </div>
    );
  if (issue === null)
    return <p className="pt-24 text-center text-atb-ink-3">issue not found</p>;

  const trophyById = new Map((state?.trophies ?? []).map((t) => [t._id, t]));
  const taskById = new Map((state?.tasks ?? []).map((t) => [t._id, t]));
  const evidence = (issue.evidence ?? []).filter((e) => e.trophyId);

  return (
    <div className="pt-12 max-w-4xl">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <div className="flex items-center gap-2 text-xs text-atb-ink-3 mb-3">
          <Link href="/atb/issues" className="hover:text-atb-ink transition-colors">
            Issues
          </Link>
          <span>/</span>
          <span className="font-atb-mono">{issue._id.slice(0, 12)}…</span>
        </div>
        <h1 className="font-atb-serif text-2xl sm:text-[1.7rem] font-semibold tracking-tight leading-snug">
          {issue.title}
        </h1>
        <div className="flex flex-wrap items-center gap-2.5 mt-4">
          <KindChip kind={issue.kind} />
          {issue.category && (
            <span className="text-xs text-atb-ink-3">{issue.category}</span>
          )}
          <StatusPill status={issueStatusLabel(issue)} />
          {(issue.linearSyncStatus ?? issue.notionSyncStatus) && (
            <span className="text-xs text-atb-ink-3">
              board: {issue.linearSyncStatus ?? issue.notionSyncStatus}
            </span>
          )}
          {issue.prUrl && (
            <a
              href={issue.prUrl}
              target="_blank"
              rel="noreferrer"
              className="text-xs text-atb-accent-deep underline underline-offset-2"
            >
              PR{issue.prNumber ? ` #${issue.prNumber}` : ""}
            </a>
          )}
          {issue.checkState && (
            <span className="text-xs text-atb-ink-3">ci: {issue.checkState}</span>
          )}
          {issue.bamlVersion && (
            <span className="font-atb-mono text-[11px] text-atb-ink-3">
              baml {issue.bamlVersion.replace(/^baml-language-/, "")}
            </span>
          )}
          {(issue.skillUsed || issue.skillVersion) && (
            <span className="font-atb-mono text-[11px] text-atb-ink-3">
              skill {skillRepoLabel(issue.skillUsed) ?? ""}
              {issue.skillVersion ? `@${issue.skillVersion.slice(0, 7)}` : ""}
            </span>
          )}
          {issue.coderabbitState && issue.coderabbitState !== "none" && (
            <span className="text-xs text-atb-ink-3">
              coderabbit: {issue.coderabbitState}
            </span>
          )}
          <span className="text-xs text-atb-ink-3">
            first seen {wallClock(issue.firstSeenAt)} · last{" "}
            {timeAgo(issue.lastSeenAt, now)}
          </span>
        </div>
        {(issue.brokeIn || issue.fixedIn || issue.verifiedAt) && (
          <div className="flex flex-wrap items-center gap-2 mt-3 text-[11px]">
            {issue.brokeIn && (
              <span className="font-atb-mono px-2.5 py-1 rounded-full bg-atb-rust-soft/60 text-atb-rust">
                broke in {issue.brokeIn}
              </span>
            )}
            {issue.fixedIn && (
              <span className="font-atb-mono px-2.5 py-1 rounded-full bg-atb-accent-soft text-atb-accent-deep">
                fixed in {issue.fixedIn}
              </span>
            )}
            {issue.verifiedAt && (
              <span className="text-atb-ink-3">
                last verified {timeAgo(issue.verifiedAt, now)}
                {issue.verifyBamlVersion ? ` against ${issue.verifyBamlVersion}` : ""}
              </span>
            )}
          </div>
        )}
      </motion.div>

      <Reveal className="mt-8">
        <Card className="px-6 py-2">
          <Markdown>{issue.description}</Markdown>
        </Card>
      </Reveal>

      {issue.suggestion && (
        <Reveal className="mt-6">
          <Card className="px-6 py-5 border-atb-accent/30 bg-atb-accent-soft/30">
            <p className="text-[11px] uppercase tracking-wider text-atb-accent-deep font-semibold mb-2">
              Suggested fix
            </p>
            <Markdown>{issue.suggestion}</Markdown>
          </Card>
        </Reveal>
      )}

      {issue.repro && (
        <Reveal className="mt-6">
          <SectionHeader title="Minimal repro" />
          <CodeView
            path="repro.baml"
            content={issue.repro}
            className="rounded-2xl"
          />
        </Reveal>
      )}

      {evidence.length > 0 && (
        <Reveal className="mt-10">
          <SectionHeader
            title={`Evidence (${evidence.length})`}
            hint="the runs that hit this"
          />
          <div className="space-y-2">
            {evidence.map((e, i) => {
              const trophy = trophyById.get(e.trophyId!);
              const task = trophy ? taskById.get(trophy.taskId) : undefined;
              return (
                <Link
                  key={i}
                  href={`/atb/runs/${e.trophyId}${
                    e.turn_index != null ? `#turn-${e.turn_index}` : ""
                  }`}
                  className="block group"
                >
                  <Card className="px-5 py-3.5 flex items-center gap-4 hover:border-atb-accent/50 transition-colors">
                    <span className="text-sm text-atb-ink line-clamp-1 flex-1 group-hover:text-atb-accent-deep transition-colors">
                      {task?.prompt ?? e.trophyId}
                    </span>
                    {trophy && (
                      <>
                        <span className="text-xs font-atb-mono text-atb-ink-3 shrink-0">
                          {usd(trophy.metrics?.estimated_cost_usd)}
                        </span>
                        <StatusPill status={trophy.outcome} />
                      </>
                    )}
                  </Card>
                </Link>
              );
            })}
          </div>
        </Reveal>
      )}
    </div>
  );
}
