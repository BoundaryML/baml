"use client";

// Shared primitives: reveal-on-scroll, count-up numbers, status pills,
// section headers, skeletons. The motion language is Anthropic-ish:
// small upward fades with a long expressive ease, nothing bouncy.

import {
  motion,
  useInView,
  useSpring,
  useTransform,
} from "framer-motion";
import { useEffect, useRef } from "react";
import Link from "next/link";

export const EASE: [number, number, number, number] = [0.22, 1, 0.36, 1];

export function Reveal({
  children,
  delay = 0,
  className,
}: {
  children: React.ReactNode;
  delay?: number;
  className?: string;
}) {
  return (
    <motion.div
      className={className}
      initial={{ opacity: 0, y: 16 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-40px" }}
      transition={{ duration: 0.7, delay, ease: EASE }}
    >
      {children}
    </motion.div>
  );
}

export function Stagger({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <motion.div
      className={className}
      initial="hidden"
      whileInView="show"
      viewport={{ once: true, margin: "-40px" }}
      variants={{
        hidden: {},
        show: { transition: { staggerChildren: 0.07 } },
      }}
    >
      {children}
    </motion.div>
  );
}

export function StaggerItem({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <motion.div
      className={className}
      variants={{
        hidden: { opacity: 0, y: 14 },
        show: { opacity: 1, y: 0, transition: { duration: 0.6, ease: EASE } },
      }}
    >
      {children}
    </motion.div>
  );
}

/** Animated number that counts up when it scrolls into view or changes. */
export function CountUp({
  value,
  format,
  className,
}: {
  value: number;
  format?: (v: number) => string;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const inView = useInView(ref, { once: true, margin: "-40px" });
  const spring = useSpring(0, { stiffness: 60, damping: 18 });
  const display = useTransform(spring, (v) =>
    format ? format(v) : Math.round(v).toLocaleString(),
  );
  useEffect(() => {
    if (inView) spring.set(value);
  }, [inView, value, spring]);
  return <motion.span ref={ref} className={className}>{display}</motion.span>;
}

// ---- status colors ----

const PILL: Record<string, string> = {
  // outcomes
  success: "bg-atb-olive-soft text-atb-olive",
  partial: "bg-atb-amber-soft text-atb-amber",
  failed: "bg-atb-rust-soft text-atb-rust",
  feedback: "bg-atb-slate-soft text-atb-slate",
  quota_skipped: "bg-atb-oat text-atb-ink-3",
  // queue states
  queued: "bg-atb-oat text-atb-ink-2",
  pending: "bg-atb-oat text-atb-ink-2",
  running: "bg-atb-accent-soft text-atb-accent-deep",
  building: "bg-atb-accent-soft text-atb-accent-deep",
  comparing: "bg-atb-accent-soft text-atb-accent-deep",
  deduping: "bg-atb-accent-soft text-atb-accent-deep",
  generating: "bg-atb-accent-soft text-atb-accent-deep",
  done: "bg-atb-olive-soft text-atb-olive",
  ready: "bg-atb-olive-soft text-atb-olive",
  // issue lifecycle
  reported: "bg-atb-amber-soft text-atb-amber",
  fixed: "bg-atb-accent-soft text-atb-accent-deep",
  open: "bg-atb-amber-soft text-atb-amber",
  confirmed: "bg-atb-accent-soft text-atb-accent-deep",
  approved: "bg-atb-slate-soft text-atb-slate",
  fixing: "bg-atb-slate-soft text-atb-slate",
  cursor: "bg-atb-slate-soft text-atb-slate",
  dispatching: "bg-atb-slate-soft text-atb-slate",
  tocursor: "bg-atb-slate-soft text-atb-slate",
  prprep: "bg-atb-accent-soft text-atb-accent-deep",
  "pr open": "bg-atb-accent-soft text-atb-accent-deep",
  pr_ready: "bg-atb-olive-soft text-atb-olive",
  "pr ready": "bg-atb-olive-soft text-atb-olive",
  needs_human: "bg-atb-amber-soft text-atb-amber",
  "needs human": "bg-atb-amber-soft text-atb-amber",
  redraft: "bg-atb-amber-soft text-atb-amber",
  redrafting: "bg-atb-amber-soft text-atb-amber",
  verifying: "bg-atb-accent-soft text-atb-accent-deep",
  closed: "bg-atb-olive-soft text-atb-olive",
  rejected: "bg-atb-oat text-atb-ink-3",
  // workers
  busy: "bg-atb-accent-soft text-atb-accent-deep",
  idle: "bg-atb-oat text-atb-ink-2",
  offline: "bg-atb-oat text-atb-ink-3",
};

const LIVE_STATES = new Set([
  "running",
  "building",
  "comparing",
  "deduping",
  "generating",
  "syncing",
  "busy",
  "dispatching",
  "redrafting",
  "verifying",
]);

export function StatusPill({ status }: { status: string }) {
  const cls = PILL[status] ?? "bg-atb-oat text-atb-ink-2";
  const live = LIVE_STATES.has(status);
  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium tracking-wide ${cls}`}
    >
      {live && (
        <span className="relative inline-flex w-1.5 h-1.5 rounded-full bg-current atb-pulse-ring" />
      )}
      {status.replace(/_/g, " ")}
    </span>
  );
}

const OUTCOME_ICON: Record<string, string> = {
  success: "🏆",
  partial: "◐",
  failed: "✕",
  feedback: "✎",
  quota_skipped: "⏸",
};

/** A run's verdict, popped in with a small spring. */
export function OutcomeBadge({ outcome }: { outcome: string }) {
  const cls = PILL[outcome] ?? "bg-atb-oat text-atb-ink-2";
  return (
    <motion.span
      initial={{ scale: 0.6, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      transition={{ type: "spring", stiffness: 320, damping: 18, delay: 0.15 }}
      className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-semibold tracking-wide ${cls}`}
    >
      <span aria-hidden>{OUTCOME_ICON[outcome] ?? "•"}</span>
      {outcome.replace(/_/g, " ")}
    </motion.span>
  );
}

export function KindChip({ kind }: { kind: string }) {
  return (
    <span
      className={`inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium border ${
        kind === "skill"
          ? "border-atb-slate/30 text-atb-slate"
          : "border-atb-accent/30 text-atb-accent-deep"
      }`}
    >
      {kind}
    </span>
  );
}

export function SectionHeader({
  title,
  hint,
  action,
}: {
  title: string;
  hint?: string;
  action?: { href: string; label: string };
}) {
  return (
    <div className="flex items-end justify-between mb-4">
      <div>
        <h2 className="font-atb-serif text-xl font-semibold tracking-tight">
          {title}
        </h2>
        {hint && <p className="text-sm text-atb-ink-3 mt-0.5">{hint}</p>}
      </div>
      {action && (
        <Link
          href={action.href}
          className="text-sm text-atb-accent-deep hover:text-atb-accent transition-colors"
        >
          {action.label} →
        </Link>
      )}
    </div>
  );
}

export function Card({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={`bg-atb-ivory/60 border border-atb-line rounded-2xl ${className}`}
    >
      {children}
    </div>
  );
}

export function Skeleton({ className = "" }: { className?: string }) {
  return (
    <div className={`bg-atb-oat/70 rounded-lg atb-blink-soft ${className}`} />
  );
}

export function EmptyState({ label }: { label: string }) {
  return (
    <div className="py-12 text-center text-sm text-atb-ink-3 font-atb-serif italic">
      {label}
    </div>
  );
}
