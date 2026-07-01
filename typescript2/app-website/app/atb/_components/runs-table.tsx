"use client";

import Link from "next/link";
import { motion } from "framer-motion";
import type { SlimTask, SlimTrophy } from "@/app/atb/_lib/types";
import { duration, timeAgo, usd } from "@/app/atb/_lib/format";
import { EASE, StatusPill, EmptyState, Skeleton } from "@/app/atb/_components/ui";
import { useNow } from "@/app/atb/_components/use-now";

export type RunRow = {
  trophy: SlimTrophy;
  task?: SlimTask;
};

/** Join slim trophies to their tasks, skipping cohort comparison reports. */
export function joinRuns(
  trophies: SlimTrophy[] | undefined,
  tasks: SlimTask[] | undefined,
): RunRow[] | undefined {
  if (!trophies) return undefined;
  const byId = new Map((tasks ?? []).map((t) => [t._id, t]));
  return trophies
    .filter((t) => !t.isCohortReport)
    .map((trophy) => ({ trophy, task: byId.get(trophy.taskId) }));
}

export function RunsTable({
  runs,
  limit,
}: {
  runs: RunRow[] | undefined;
  limit?: number;
}) {
  const now = useNow();
  if (!runs) {
    return (
      <div className="space-y-2">
        {[...Array(4)].map((_, i) => (
          <Skeleton key={i} className="h-14" />
        ))}
      </div>
    );
  }
  const rows = limit ? runs.slice(0, limit) : runs;
  if (rows.length === 0) return <EmptyState label="no runs yet" />;

  return (
    <div className="border border-atb-line rounded-2xl overflow-hidden bg-atb-ivory/40">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-[11px] uppercase tracking-wider text-atb-ink-3 border-b border-atb-line">
            <th className="text-left font-medium px-4 py-2.5">Task</th>
            <th className="text-left font-medium px-3 py-2.5 w-24">Outcome</th>
            <th className="text-right font-medium px-3 py-2.5 w-16">Turns</th>
            <th className="text-right font-medium px-3 py-2.5 w-20">Cost</th>
            <th className="text-right font-medium px-3 py-2.5 w-20 hidden sm:table-cell">
              Wall
            </th>
            <th className="text-right font-medium px-3 py-2.5 w-20 hidden md:table-cell">
              Findings
            </th>
            <th className="text-right font-medium px-4 py-2.5 w-24">When</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ trophy, task }, i) => (
            <motion.tr
              key={trophy._id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.45, delay: Math.min(i * 0.04, 0.5), ease: EASE }}
              className="border-b border-atb-line/60 last:border-0 hover:bg-atb-oat/40 transition-colors group"
            >
              <td className="px-4 py-3">
                <Link href={`/atb/runs/${trophy._id}`} className="block">
                  <span className="line-clamp-2 text-atb-ink group-hover:text-atb-accent-deep transition-colors leading-snug">
                    {task?.prompt ?? "(task not found)"}
                  </span>
                  <span className="mt-1 flex items-center gap-2 text-[11px] text-atb-ink-3 font-atb-mono">
                    <span>{task?.source ?? "?"}</span>
                    {task?.skillRef && <span>· {task.skillRef}</span>}
                  </span>
                </Link>
              </td>
              <td className="px-3 py-3">
                <StatusPill status={trophy.outcome} />
              </td>
              <td className="px-3 py-3 text-right font-atb-mono text-atb-ink-2">
                {trophy.metrics?.turns ?? "—"}
              </td>
              <td className="px-3 py-3 text-right font-atb-mono text-atb-ink-2">
                {usd(trophy.metrics?.estimated_cost_usd)}
              </td>
              <td className="px-3 py-3 text-right font-atb-mono text-atb-ink-2 hidden sm:table-cell">
                {duration(trophy.metrics?.wall_clock_ms)}
              </td>
              <td className="px-3 py-3 text-right font-atb-mono hidden md:table-cell">
                {trophy.findingsCount > 0 ? (
                  <span className="text-atb-accent-deep">{trophy.findingsCount}</span>
                ) : (
                  <span className="text-atb-ink-3">0</span>
                )}
              </td>
              <td className="px-4 py-3 text-right text-atb-ink-3 text-xs whitespace-nowrap">
                {timeAgo(trophy.createdAt, now)}
              </td>
            </motion.tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
