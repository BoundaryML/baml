import type { Cohort, SlimTask, SlimTrophy } from "@/app/atb/_lib/types";

// One arena lane: a skill variant's run inside a cohort.
export type Lane = {
  taskId: string;
  skillRef: string;
  skillStorageId: string | null;
  trophyId: string | null;
  outcome: string | null;
  status: string;
  turns: number | null;
  costUsd: number | null;
  wallMs: number | null;
  findings: number;
};

export function cohortLanes(
  cohort: Cohort,
  tasks: SlimTask[],
  trophies: SlimTrophy[],
): Lane[] {
  const members = tasks.filter((t) => t.cohortId === cohort._id);
  return members.map((m) => {
    const tr = trophies.find(
      (t) => t.taskId === m._id && !t.isCohortReport,
    );
    return {
      taskId: m._id,
      skillRef: m.skillRef ?? "main",
      skillStorageId: m.skillStorageId ?? null,
      trophyId: tr?._id ?? null,
      outcome: tr?.outcome ?? null,
      status: m.status,
      turns: tr?.metrics?.turns ?? null,
      costUsd: tr?.metrics?.estimated_cost_usd ?? null,
      wallMs: tr?.metrics?.wall_clock_ms ?? null,
      findings: tr?.findingsCount ?? 0,
    };
  });
}

/** Rank lanes: successes first, then cheapest, then fewest turns. */
export function rankLanes(lanes: Lane[]): Lane[] {
  return [...lanes].sort((a, b) => {
    const sa = a.outcome === "success" ? 0 : 1;
    const sb = b.outcome === "success" ? 0 : 1;
    if (sa !== sb) return sa - sb;
    if ((a.costUsd ?? Infinity) !== (b.costUsd ?? Infinity))
      return (a.costUsd ?? Infinity) - (b.costUsd ?? Infinity);
    return (a.turns ?? Infinity) - (b.turns ?? Infinity);
  });
}

/**
 * The judge's pick, when the report summary names a variant: the first
 * skillRef mentioned wins. Falls back to the rank heuristic.
 */
export function winnerRef(
  lanes: Lane[],
  judgeSummary?: string | null,
): string | null {
  if (lanes.length === 0) return null;
  if (judgeSummary) {
    let best: { ref: string; at: number } | null = null;
    for (const l of lanes) {
      const at = judgeSummary.indexOf(l.skillRef);
      if (at >= 0 && (best === null || at < best.at))
        best = { ref: l.skillRef, at };
    }
    if (best) return best.ref;
  }
  const ranked = rankLanes(lanes);
  return ranked[0].outcome === "success" ? ranked[0].skillRef : null;
}

export const MEDALS = ["🥇", "🥈", "🥉"];
