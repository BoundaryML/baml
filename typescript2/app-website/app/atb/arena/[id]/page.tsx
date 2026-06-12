"use client";

// Arena detail: a podium for the placed variants, the full scoreboard with
// animated bars, and the judge's verdict.

import Link from "next/link";
import { use, useState } from "react";
import { motion } from "framer-motion";
import { useAtbState, useDoc } from "@/app/atb/_lib/api";
import { cohortLanes, rankLanes, winnerRef, MEDALS, type Lane } from "@/app/atb/_lib/arena";
import type { Cohort, Trophy } from "@/app/atb/_lib/types";
import { duration, usd, wallClock } from "@/app/atb/_lib/format";
import {
  Card,
  EASE,
  Reveal,
  SectionHeader,
  Skeleton,
  StatusPill,
} from "@/app/atb/_components/ui";
import { Markdown } from "@/app/atb/_components/markdown";
import { CodeView } from "@/app/atb/_components/code-view";

export default function CohortPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const cohort = useDoc<Cohort>("cohorts", id);
  const state = useAtbState();
  const report = useDoc<Trophy>("trophies", cohort?.reportTrophyId ?? null);

  if (cohort === undefined)
    return (
      <div className="pt-12 space-y-6">
        <Skeleton className="h-5 w-40" />
        <Skeleton className="h-14 w-3/4" />
        <Skeleton className="h-48" />
      </div>
    );
  if (cohort === null)
    return <p className="pt-24 text-center text-atb-ink-3">arena not found</p>;

  const lanes = state
    ? rankLanes(cohortLanes(cohort, state.tasks, state.trophies))
    : [];
  const winner = winnerRef(lanes, report?.summary);
  // judge's pick first, the rest keep heuristic order
  const ordered = [
    ...lanes.filter((l) => l.skillRef === winner),
    ...lanes.filter((l) => l.skillRef !== winner),
  ];

  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <div className="flex items-center gap-2 text-xs text-atb-ink-3 mb-3">
          <Link href="/atb/arena" className="hover:text-atb-ink transition-colors">
            Arena
          </Link>
          <span>/</span>
          <span className="font-atb-mono">{cohort._id.slice(0, 12)}…</span>
        </div>
        <h1 className="font-atb-serif text-2xl sm:text-[1.7rem] font-semibold tracking-tight leading-snug max-w-3xl">
          {cohort.prompt}
        </h1>
        <div className="flex items-center gap-3 mt-4">
          <StatusPill status={cohort.status} />
          <span className="text-xs text-atb-ink-3">
            {wallClock(cohort.createdAt)}
          </span>
          <span className="text-xs text-atb-ink-3">
            {lanes.length} variants
          </span>
        </div>
      </motion.div>

      {/* ---- podium ---- */}
      {cohort.status === "done" && ordered.length >= 2 && (
        <Reveal className="mt-10">
          <Podium lanes={ordered.slice(0, 3)} />
        </Reveal>
      )}

      {/* ---- scoreboard ---- */}
      <Reveal className="mt-10">
        <SectionHeader
          title="Scoreboard"
          hint="same task, same nightly, one agent per skill branch"
        />
        <div className="space-y-2">
          {ordered.map((l, i) => (
            <LaneRow
              key={l.taskId}
              lane={l}
              place={i}
              isWinner={winner != null && l.skillRef === winner}
              maxCost={Math.max(...lanes.map((x) => x.costUsd ?? 0), 0.01)}
              maxTurns={Math.max(...lanes.map((x) => x.turns ?? 0), 1)}
            />
          ))}
        </div>
      </Reveal>

      {/* ---- skill files ---- */}
      {ordered.some((l) => l.skillStorageId) && (
        <Reveal className="mt-10">
          <SectionHeader
            title="Skill files"
            hint="the exact skill text each variant onboarded with"
          />
          <div className="space-y-3">
            {ordered
              .filter((l) => l.skillStorageId)
              .map((l) => (
                <SkillFile
                  key={l.taskId}
                  skillRef={l.skillRef}
                  storageId={l.skillStorageId!}
                  isWinner={winner != null && l.skillRef === winner}
                />
              ))}
          </div>
        </Reveal>
      )}

      {/* ---- judge's verdict ---- */}
      {report?.summary && (
        <Reveal className="mt-10">
          <SectionHeader title="Verdict" hint="the judge's summary" />
          <Card className="px-6 py-5 border-atb-accent/30 bg-atb-accent-soft/30">
            <p className="font-atb-serif text-[1.05rem] leading-relaxed text-atb-ink-2">
              {report.summary}
            </p>
          </Card>
        </Reveal>
      )}
      {report?.reportMd && (
        <Reveal className="mt-6">
          <Card className="px-6 py-2">
            <Markdown>{report.reportMd}</Markdown>
          </Card>
          {cohort.reportTrophyId && (
            <Link
              href={`/atb/runs/${cohort.reportTrophyId}`}
              className="inline-block mt-3 text-sm text-atb-accent-deep hover:text-atb-accent transition-colors"
            >
              full report record →
            </Link>
          )}
        </Reveal>
      )}
    </div>
  );
}

// Heights for 1st / 2nd / 3rd podium steps.
const STEP_H = [120, 88, 64];
const STEP_ORDER = [1, 0, 2]; // visual order: 2nd, 1st, 3rd

function Podium({ lanes }: { lanes: Lane[] }) {
  const slots = STEP_ORDER.filter((i) => i < lanes.length);
  return (
    <div className="flex items-end justify-center gap-3 sm:gap-6 pt-6">
      {slots.map((place) => {
        const l = lanes[place];
        return (
          <div
            key={l.taskId}
            className="flex flex-col items-center w-40 sm:w-48"
          >
            <motion.span
              initial={{ scale: 0, rotate: -20 }}
              whileInView={{ scale: 1, rotate: 0 }}
              viewport={{ once: true }}
              transition={{
                type: "spring",
                stiffness: 260,
                damping: 14,
                delay: 0.4 + place * 0.15,
              }}
              className="text-3xl mb-2"
            >
              {MEDALS[place]}
            </motion.span>
            <LinkOrSpan lane={l}>
              <span
                className={`font-atb-mono text-xs text-center leading-snug break-all ${
                  place === 0 ? "text-atb-accent-deep font-semibold" : "text-atb-ink-2"
                }`}
              >
                {l.skillRef}
              </span>
            </LinkOrSpan>
            <span className="text-[11px] text-atb-ink-3 font-atb-mono mt-1 mb-2">
              {l.turns ?? "?"} turns · {usd(l.costUsd)}
            </span>
            <motion.div
              initial={{ height: 0 }}
              whileInView={{ height: STEP_H[place] }}
              viewport={{ once: true }}
              transition={{ duration: 0.7, delay: place * 0.15, ease: EASE }}
              className={`w-full rounded-t-xl border border-b-0 ${
                place === 0
                  ? "bg-atb-accent-soft border-atb-accent/40"
                  : "bg-atb-ivory border-atb-line"
              } flex items-start justify-center pt-2`}
            >
              <span
                className={`font-atb-serif text-lg font-semibold ${
                  place === 0 ? "text-atb-accent-deep" : "text-atb-ink-3"
                }`}
              >
                {place + 1}
              </span>
            </motion.div>
          </div>
        );
      })}
    </div>
  );
}

function LinkOrSpan({
  lane,
  children,
}: {
  lane: Lane;
  children: React.ReactNode;
}) {
  if (!lane.trophyId) return <>{children}</>;
  return (
    <Link
      href={`/atb/runs/${lane.trophyId}`}
      className="hover:opacity-70 transition-opacity"
    >
      {children}
    </Link>
  );
}

function LaneRow({
  lane,
  place,
  isWinner,
  maxCost,
  maxTurns,
}: {
  lane: Lane;
  place: number;
  isWinner: boolean;
  maxCost: number;
  maxTurns: number;
}) {
  const inner = (
    <div
      className={`border rounded-2xl px-5 py-4 transition-colors ${
        isWinner
          ? "bg-atb-accent-soft/40 border-atb-accent/40"
          : "bg-atb-ivory/60 border-atb-line hover:border-atb-line-strong"
      }`}
    >
      <div className="flex items-center gap-3">
        <span className="w-6 text-center text-lg">
          {place < 3 && lane.outcome === "success" ? (
            <span className={isWinner ? "inline-block bob" : ""}>
              {MEDALS[place]}
            </span>
          ) : (
            <span className="text-atb-ink-3 text-sm">{place + 1}</span>
          )}
        </span>
        <span
          className={`font-atb-mono text-sm ${
            isWinner ? "text-atb-accent-deep font-semibold" : "text-atb-ink"
          }`}
        >
          {lane.skillRef}
        </span>
        <StatusPill status={lane.outcome ?? lane.status} />
        <span className="ml-auto font-atb-mono text-xs text-atb-ink-3">
          {lane.findings} findings · {duration(lane.wallMs)}
        </span>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-4">
        <MiniBar
          label="turns"
          text={`${lane.turns ?? "?"}`}
          frac={(lane.turns ?? 0) / maxTurns}
          highlight={isWinner}
          delay={0.1 + place * 0.08}
        />
        <MiniBar
          label="cost"
          text={usd(lane.costUsd)}
          frac={(lane.costUsd ?? 0) / maxCost}
          highlight={isWinner}
          delay={0.18 + place * 0.08}
        />
      </div>
    </div>
  );
  if (!lane.trophyId) return inner;
  return (
    <Link href={`/atb/runs/${lane.trophyId}`} className="block">
      {inner}
    </Link>
  );
}

function MiniBar({
  label,
  text,
  frac,
  highlight,
  delay,
}: {
  label: string;
  text: string;
  frac: number;
  highlight: boolean;
  delay: number;
}) {
  return (
    <div>
      <div className="flex justify-between text-[10px] uppercase tracking-wider text-atb-ink-3 mb-1">
        <span>{label}</span>
        <span className="font-atb-mono normal-case">{text}</span>
      </div>
      <div className="h-1.5 rounded-full bg-atb-oat/70 overflow-hidden">
        <motion.div
          initial={{ width: 0 }}
          whileInView={{ width: `${Math.max(4, frac * 100)}%` }}
          viewport={{ once: true }}
          transition={{ duration: 0.8, delay, ease: EASE }}
          className={`h-full rounded-full ${
            highlight ? "bg-atb-accent" : "bg-atb-accent/30"
          }`}
        />
      </div>
    </div>
  );
}

/** One variant's SKILL.md snapshot, lazy-loaded when opened. */
function SkillFile({
  skillRef,
  storageId,
  isWinner,
}: {
  skillRef: string;
  storageId: string;
  isWinner: boolean;
}) {
  const [text, setText] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  const load = () => {
    if (text != null || failed) return;
    fetch(`/api/atb/transcript/${storageId}`)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`${r.status}`))))
      .then(setText)
      .catch(() => setFailed(true));
  };

  return (
    <details
      className={`border rounded-2xl overflow-hidden group ${
        isWinner ? "bg-atb-accent-soft/40 border-atb-accent/40" : "bg-atb-ivory/50 border-atb-line"
      }`}
      onToggle={(e) => (e.target as HTMLDetailsElement).open && load()}
    >
      <summary className="px-5 py-3 cursor-pointer hover:bg-atb-oat/40 transition-colors list-none flex items-center gap-2.5">
        <span className="text-atb-ink-3 text-[10px] transition-transform group-open:rotate-90">
          ▶
        </span>
        <span
          className={`font-atb-mono text-sm ${
            isWinner ? "text-atb-accent-deep font-semibold" : "text-atb-ink-2"
          }`}
        >
          {skillRef}
        </span>
        {isWinner && <span className="text-sm">🥇</span>}
        <span className="ml-auto text-xs text-atb-ink-3 font-atb-mono">
          SKILL.md
        </span>
      </summary>
      {failed ? (
        <p className="px-5 py-4 text-sm text-atb-ink-3">skill file unavailable</p>
      ) : text == null ? (
        <div className="m-4 h-24 bg-atb-oat/70 rounded-xl atb-blink-soft" />
      ) : (
        <CodeView path="SKILL.md" content={text} className="max-h-[60vh]" />
      )}
    </details>
  );
}
