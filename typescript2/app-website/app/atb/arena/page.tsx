"use client";

// The skill arena: one task, run once per skill variant, judged. Each
// cohort card is a small race chart: a lane per variant, bar length is
// cost, the winner gets the medal.

import Link from "next/link";
import { motion } from "framer-motion";
import { useAtbState } from "@/app/atb/_lib/api";
import { cohortLanes, rankLanes, winnerRef, MEDALS } from "@/app/atb/_lib/arena";
import type { Cohort } from "@/app/atb/_lib/types";
import { timeAgo, usd } from "@/app/atb/_lib/format";
import {
  EASE,
  EmptyState,
  Skeleton,
  Stagger,
  StaggerItem,
  StatusPill,
} from "@/app/atb/_components/ui";
import { useNow } from "@/app/atb/_components/use-now";

export default function ArenaPage() {
  const now = useNow();
  const state = useAtbState();
  const cohorts = state?.cohorts;

  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <h1 className="font-atb-serif text-3xl font-semibold tracking-tight">
          Skill arena
        </h1>
        <p className="text-atb-ink-3 mt-1 text-sm">
          One task, run once per skill variant. A judge compares the runs and
          picks a winner.
          {cohorts ? ` ${cohorts.length} arenas so far.` : ""}
        </p>
      </motion.div>

      {!cohorts ? (
        <div className="space-y-3 mt-8">
          {[...Array(3)].map((_, i) => (
            <Skeleton key={i} className="h-40" />
          ))}
        </div>
      ) : cohorts.length === 0 ? (
        <EmptyState label="no arenas yet" />
      ) : (
        <Stagger className="grid md:grid-cols-2 gap-4 mt-8">
          {cohorts.map((c, i) => (
            <StaggerItem key={c._id}>
              <ArenaCard
                cohort={c}
                index={cohorts.length - i}
                now={now}
              />
            </StaggerItem>
          ))}
        </Stagger>
      )}
    </div>
  );
}

function ArenaCard({
  cohort,
  index,
  now,
}: {
  cohort: Cohort;
  index: number;
  now: number;
}) {
  const state = useAtbState();
  const lanes = state
    ? rankLanes(cohortLanes(cohort, state.tasks, state.trophies))
    : [];
  const winner = winnerRef(lanes);
  const maxCost = Math.max(...lanes.map((l) => l.costUsd ?? 0), 0.01);

  return (
    <Link href={`/atb/arena/${cohort._id}`} className="block h-full group">
      <motion.div
        whileHover={{ y: -3 }}
        transition={{ type: "spring", stiffness: 300, damping: 24 }}
        className="bg-atb-ivory/60 border border-atb-line rounded-2xl px-6 py-5 h-full hover:border-atb-accent/50 transition-colors"
      >
        <div className="flex items-center gap-3 mb-2.5">
          <span className="font-atb-mono text-[11px] text-atb-ink-3">
            arena #{index}
          </span>
          <StatusPill status={cohort.status} />
          <span className="ml-auto text-xs text-atb-ink-3">
            {timeAgo(cohort.createdAt, now)}
          </span>
        </div>
        <p className="font-atb-serif text-[15px] text-atb-ink leading-snug line-clamp-2 group-hover:text-atb-accent-deep transition-colors">
          {cohort.prompt}
        </p>

        {/* race lanes */}
        <div className="mt-4 space-y-2">
          {lanes.map((l, li) => {
            const isWinner = winner != null && l.skillRef === winner;
            const frac =
              l.costUsd != null ? Math.max(0.08, l.costUsd / maxCost) : 0;
            return (
              <div key={l.taskId} className="flex items-center gap-2.5">
                <span className="w-5 text-center text-sm shrink-0">
                  {isWinner ? (
                    <span className="inline-block atb-bob">{MEDALS[0]}</span>
                  ) : l.outcome === "failed" ? (
                    <span className="text-atb-rust text-xs">✕</span>
                  ) : (
                    <span className="text-atb-ink-3 text-xs">{li + 1}</span>
                  )}
                </span>
                <span
                  className={`font-atb-mono text-[11px] truncate w-36 shrink-0 ${
                    isWinner ? "text-atb-accent-deep font-semibold" : "text-atb-ink-2"
                  }`}
                >
                  {l.skillRef}
                </span>
                <div className="flex-1 h-2 rounded-full bg-atb-oat/70 overflow-hidden">
                  {frac > 0 && (
                    <motion.div
                      initial={{ width: 0 }}
                      whileInView={{ width: `${frac * 100}%` }}
                      viewport={{ once: true }}
                      transition={{
                        duration: 0.9,
                        delay: 0.15 + li * 0.12,
                        ease: EASE,
                      }}
                      className={`h-full rounded-full ${
                        isWinner
                          ? "bg-atb-accent"
                          : l.outcome === "failed"
                            ? "bg-atb-rust/50"
                            : "bg-atb-accent/30"
                      }`}
                    />
                  )}
                </div>
                <span className="font-atb-mono text-[11px] text-atb-ink-3 w-12 text-right shrink-0">
                  {usd(l.costUsd)}
                </span>
              </div>
            );
          })}
        </div>
      </motion.div>
    </Link>
  );
}
