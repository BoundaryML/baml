"use client";

// The agent roster: every long-lived processor in the loop, its heartbeat,
// and what it is working on right now.

import Link from "next/link";
import { motion } from "framer-motion";
import { useAtbState, workerOnline } from "@/app/atb/_lib/api";
import { timeAgo } from "@/app/atb/_lib/format";
import { EASE, Skeleton, Stagger, StaggerItem } from "@/app/atb/_components/ui";
import { useNow } from "@/app/atb/_components/use-now";

// v1 roles are hyphenated, the v2 (pure-BAML) processors register with
// underscores - both spellings resolve so the roster stays labeled across
// deployments.
const ROLE_BLURB: Record<string, string> = {
  "baml-worker": "runs benchmark tasks with the canary baml on PATH",
  baml_worker: "runs benchmark tasks with the canary baml on PATH",
  "baml-builder": "builds each nightly baml CLI from source",
  baml_builder: "builds each nightly baml CLI from source",
  "baml-dedup": "merges run findings into deduplicated issues",
  baml_dedup: "merges run findings into deduplicated issues",
  "baml-redraft": "redrafts issues sent back from review",
  baml_redraft: "redrafts issues sent back from review",
  "notion-push": "syncs confirmed issues to the Notion boards",
  linear_push: "mirrors issues onto the Linear board",
  "fix-dispatch": "hands approved issues to Cursor cloud agents",
  fix_dispatch: "hands approved issues to Cursor cloud agents",
  "cohort-compare": "judges skill arena cohorts and writes the report",
  cohort_compare: "judges skill arena cohorts and writes the report",
  "changelog-worker": "drafts and critiques release changelogs",
  changelog_worker: "drafts and critiques release changelogs",
  "bug-verify": "re-checks reported bugs on the latest nightly and closes fixed ones",
  bug_verify: "re-checks reported bugs on the latest nightly and closes fixed ones",
};

export default function AgentsPage() {
  const now = useNow(5_000);
  const state = useAtbState();
  const workers = state?.workers;

  // resolve a worker's currentItemId to a label + link
  const resolve = (
    id?: string | null,
  ): { label: string; href: string } | null => {
    if (!id || !state) return null;
    const task = state.tasks.find((t) => t._id === id);
    if (task) {
      const trophy = state.trophies.find((tr) => tr.taskId === id);
      return {
        label: task.prompt,
        href: trophy ? `/atb/runs/${trophy._id}` : "/runs",
      };
    }
    const trophy = state.trophies.find((tr) => tr._id === id);
    if (trophy) {
      const t = state.tasks.find((x) => x._id === trophy.taskId);
      return { label: t?.prompt ?? "result batch", href: `/atb/runs/${id}` };
    }
    const issue = state.issues.find((i) => i._id === id);
    if (issue) return { label: issue.title, href: `/atb/issues/${id}` };
    const cohort = state.cohorts.find((c) => c._id === id);
    if (cohort) return { label: cohort.prompt, href: `/atb/arena/${id}` };
    return null;
  };

  const sorted = workers
    ? [...workers].sort(
        (a, b) =>
          (b.status === "busy" ? 1 : 0) - (a.status === "busy" ? 1 : 0) ||
          (workerOnline(b, now) ? 1 : 0) - (workerOnline(a, now) ? 1 : 0) ||
          a.role.localeCompare(b.role),
      )
    : undefined;

  const online = (sorted ?? []).filter((w) => workerOnline(w, now)).length;
  const busy = (sorted ?? []).filter(
    (w) => w.status === "busy" && workerOnline(w, now),
  ).length;

  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <h1 className="font-atb-serif text-3xl font-semibold tracking-tight">
          Agents
        </h1>
        <p className="text-atb-ink-3 mt-1 text-sm">
          {sorted ? `${online} online, ${busy} busy.` : "loading the roster"}
        </p>
      </motion.div>

      {!sorted ? (
        <div className="grid sm:grid-cols-2 gap-4 mt-8">
          {[...Array(6)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : (
        <Stagger className="grid sm:grid-cols-2 gap-4 mt-8">
          {sorted.map((w) => {
            const on = workerOnline(w, now);
            const item = resolve(w.currentItemId);
            return (
              <StaggerItem key={w._id}>
                <div
                  className={`relative border rounded-2xl px-6 py-5 transition-all duration-300 h-full ${
                    w.status === "busy" && on
                      ? "bg-atb-accent-soft/40 border-atb-accent/40"
                      : on
                        ? "bg-atb-ivory/60 border-atb-line hover:border-atb-line-strong"
                        : "bg-atb-ivory/30 border-atb-line opacity-60"
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <span
                      className={`relative inline-flex w-2.5 h-2.5 rounded-full shrink-0 ${
                        !on
                          ? "bg-atb-line-strong"
                          : w.status === "busy"
                            ? "bg-atb-accent text-atb-accent atb-pulse-ring"
                            : "bg-atb-olive text-atb-olive atb-pulse-ring"
                      }`}
                    />
                    <h2 className="font-atb-serif text-lg font-semibold tracking-tight">
                      {w.role}
                    </h2>
                    <span className="ml-auto text-xs text-atb-ink-3">
                      {on ? w.status : "offline"}
                    </span>
                  </div>
                  <p className="mt-1.5 text-sm text-atb-ink-2 leading-relaxed">
                    {ROLE_BLURB[w.role] ?? "long-lived processor"}
                  </p>

                  {item && w.status === "busy" && (
                    <Link
                      href={item.href}
                      className="mt-3 block bg-atb-cloud/70 border border-atb-line rounded-xl px-4 py-2.5 hover:border-atb-accent/50 transition-colors"
                    >
                      <p className="text-[10px] uppercase tracking-wider text-atb-accent-deep font-medium mb-0.5">
                        working on
                      </p>
                      <p className="text-sm text-atb-ink line-clamp-2 leading-snug">
                        {item.label}
                      </p>
                    </Link>
                  )}

                  <div className="mt-3 flex items-center justify-between text-[11px] text-atb-ink-3 font-atb-mono">
                    <span className="truncate">{w.workerId}</span>
                    <span className="shrink-0 ml-3">
                      ♥ {timeAgo(w.lastHeartbeat, now)}
                    </span>
                  </div>
                </div>
              </StaggerItem>
            );
          })}
        </Stagger>
      )}
    </div>
  );
}
