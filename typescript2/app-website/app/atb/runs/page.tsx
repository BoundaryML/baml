"use client";

import { useMemo, useState } from "react";
import { motion } from "framer-motion";
import { useAtbState } from "@/app/atb/_lib/api";
import { RunsTable, joinRuns } from "@/app/atb/_components/runs-table";
import { EASE } from "@/app/atb/_components/ui";

const OUTCOMES = ["all", "success", "partial", "failed"] as const;

export default function RunsPage() {
  const state = useAtbState();
  const [outcome, setOutcome] = useState<(typeof OUTCOMES)[number]>("all");
  const [query, setQuery] = useState("");

  const allRuns = useMemo(
    () => joinRuns(state?.trophies, state?.tasks),
    [state],
  );

  const runs = useMemo(() => {
    if (!allRuns) return undefined;
    return allRuns.filter((r) => {
      if (outcome !== "all" && r.trophy.outcome !== outcome) return false;
      if (
        query &&
        !(r.task?.prompt ?? "").toLowerCase().includes(query.toLowerCase())
      )
        return false;
      return true;
    });
  }, [allRuns, outcome, query]);

  const counts = useMemo(() => {
    const c: Record<string, number> = { all: allRuns?.length ?? 0 };
    for (const r of allRuns ?? [])
      c[r.trophy.outcome] = (c[r.trophy.outcome] ?? 0) + 1;
    return c;
  }, [allRuns]);

  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <h1 className="font-atb-serif text-3xl font-semibold tracking-tight">
          Runs
        </h1>
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, delay: 0.1, ease: EASE }}
        className="flex flex-wrap items-center gap-2 mt-6 mb-5"
      >
        <div className="flex bg-atb-ivory border border-atb-line rounded-full p-0.5">
          {OUTCOMES.map((o) => (
            <button
              key={o}
              onClick={() => setOutcome(o)}
              className={`relative px-3.5 py-1 text-xs font-medium rounded-full transition-colors ${
                outcome === o ? "text-atb-cloud" : "text-atb-ink-2 hover:text-atb-ink"
              }`}
            >
              {outcome === o && (
                <motion.span
                  layoutId="runs-filter"
                  className="absolute inset-0 bg-atb-ink rounded-full"
                  transition={{ type: "spring", stiffness: 400, damping: 34 }}
                />
              )}
              <span className="relative capitalize">
                {o}
                {counts[o] != null && (
                  <span className="ml-1 opacity-60">{counts[o]}</span>
                )}
              </span>
            </button>
          ))}
        </div>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="filter prompts"
          className="ml-auto bg-atb-ivory border border-atb-line rounded-full px-4 py-1.5 text-sm w-56 placeholder:text-atb-ink-3 focus:outline-none focus:border-atb-accent/60 transition-colors"
        />
      </motion.div>

      <RunsTable runs={runs} />
    </div>
  );
}
