"use client";

import Link from "next/link";
import { useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  useAtbState,
  issueStatusLabel,
  isOpenIssueStatus,
  bamlRefLabel,
  skillRepoLabel,
} from "@/app/atb/_lib/api";
import { timeAgo } from "@/app/atb/_lib/format";
import { EASE, EmptyState, KindChip, Skeleton, StatusPill } from "@/app/atb/_components/ui";
import { useNow } from "@/app/atb/_components/use-now";

const KINDS = ["all", "language", "skill"] as const;
const STATES = ["open", "all", "closed"] as const;

export default function IssuesPage() {
  const now = useNow();
  const state = useAtbState();
  const issues = state?.issues;
  const [kind, setKind] = useState<(typeof KINDS)[number]>("all");
  const [openState, setOpenState] = useState<(typeof STATES)[number]>("open");

  const filtered = useMemo(() => {
    if (!issues) return undefined;
    return [...issues]
      .filter((i) => {
        if (kind !== "all" && i.kind !== kind) return false;
        const isOpen = isOpenIssueStatus(i.status);
        if (openState === "open" && !isOpen) return false;
        if (openState === "closed" && isOpen) return false;
        return true;
      })
      .sort((a, b) => b.lastSeenAt - a.lastSeenAt);
  }, [issues, kind, openState]);

  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <h1 className="font-atb-serif text-3xl font-semibold tracking-tight">
          Issues
        </h1>
        <p className="text-atb-ink-3 mt-1 text-sm">
          Run findings, deduplicated. Synced to Linear, dispatched to Cursor
          for fixes, and re-verified against each new baml build.
        </p>
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, delay: 0.1, ease: EASE }}
        className="flex flex-wrap gap-2 mt-6 mb-5"
      >
        <Toggle options={STATES} value={openState} onChange={setOpenState} layoutId="issues-state" />
        <Toggle options={KINDS} value={kind} onChange={setKind} layoutId="issues-kind" />
      </motion.div>

      {!filtered ? (
        <div className="space-y-2">
          {[...Array(6)].map((_, i) => (
            <Skeleton key={i} className="h-16" />
          ))}
        </div>
      ) : filtered.length === 0 ? (
        <EmptyState label="nothing here" />
      ) : (
        <div className="space-y-2">
          {filtered.map((issue, i) => (
            <motion.div
              key={issue._id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.45, delay: Math.min(i * 0.03, 0.4), ease: EASE }}
            >
              <Link href={`/atb/issues/${issue._id}`} className="block group">
                <div className="bg-atb-ivory/60 border border-atb-line rounded-2xl px-5 py-4 hover:border-atb-accent/50 transition-colors flex items-center gap-4">
                  <KindChip kind={issue.kind} />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-atb-ink leading-snug line-clamp-1 group-hover:text-atb-accent-deep transition-colors">
                      {issue.title}
                    </p>
                    <p className="text-[11px] text-atb-ink-3 mt-1">
                      {issue.category ?? "uncategorized"} ·{" "}
                      {issue.evidenceCount} evidence run
                      {issue.evidenceCount === 1 ? "" : "s"} · seen{" "}
                      {timeAgo(issue.lastSeenAt, now)}
                      {issue.fixedIn ? (
                        <span className="text-atb-accent-deep">
                          {" "}· fixed in {bamlRefLabel(issue.fixedIn)}
                        </span>
                      ) : issue.verifiedAt ? (
                        <> · verified {timeAgo(issue.verifiedAt, now)}</>
                      ) : null}
                    </p>
                    {(issue.bamlVersion || issue.skillVersion) && (
                      <p className="font-atb-mono text-[10px] text-atb-ink-3/80 mt-0.5 line-clamp-1">
                        {issue.bamlVersion
                          ? `baml ${bamlRefLabel(issue.bamlVersion)}`
                          : ""}
                        {issue.bamlVersion && issue.skillVersion ? " · " : ""}
                        {issue.skillVersion
                          ? `skill ${skillRepoLabel(issue.skillUsed) ?? ""}@${issue.skillVersion.slice(0, 7)}`
                          : ""}
                      </p>
                    )}
                  </div>
                  <StatusPill status={issueStatusLabel(issue)} />
                </div>
              </Link>
            </motion.div>
          ))}
        </div>
      )}
    </div>
  );
}

function Toggle<T extends string>({
  options,
  value,
  onChange,
  layoutId,
}: {
  options: readonly T[];
  value: T;
  onChange: (v: T) => void;
  layoutId: string;
}) {
  return (
    <div className="flex bg-atb-ivory border border-atb-line rounded-full p-0.5">
      {options.map((o) => (
        <button
          key={o}
          onClick={() => onChange(o)}
          className={`relative px-3.5 py-1 text-xs font-medium rounded-full transition-colors ${
            value === o ? "text-atb-cloud" : "text-atb-ink-2 hover:text-atb-ink"
          }`}
        >
          {value === o && (
            <motion.span
              layoutId={layoutId}
              className="absolute inset-0 bg-atb-ink rounded-full"
              transition={{ type: "spring", stiffness: 400, damping: 34 }}
            />
          )}
          <span className="relative capitalize">{o}</span>
        </button>
      ))}
    </div>
  );
}
