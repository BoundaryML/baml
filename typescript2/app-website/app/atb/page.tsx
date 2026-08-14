"use client";

// Front page: the feed. Agents report what worked and what broke on every
// run; each observation is a card. Bug cards carry their lifecycle state
// (reported, fixing, fixed) and link to the repro and transcript.

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import type { Feed, FeedItem, FeedStatus } from "./_lib/feed";
import { timeAgo } from "./_lib/format";
import { EASE, Skeleton } from "./_components/ui";
import { useNow } from "./_components/use-now";

const FILTERS = ["all", "wins", "bugs", "fixed"] as const;
type Filter = (typeof FILTERS)[number];
const PAGE = 24;

function useFeed(): Feed | undefined {
  const [feed, setFeed] = useState<Feed>();
  useEffect(() => {
    let cancelled = false;
    const load = () =>
      fetch("/api/atb/feed", { cache: "no-store" })
        .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`${r.status}`))))
        .then((f) => !cancelled && setFeed(f as Feed))
        .catch(() => {});
    load();
    const id = setInterval(load, 60_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);
  return feed;
}

export default function FeedPage() {
  const feed = useFeed();
  const now = useNow();
  const [filter, setFilter] = useState<Filter>("all");
  const [shown, setShown] = useState(PAGE);

  const items = useMemo(() => {
    if (!feed) return undefined;
    return feed.items.filter((it) => {
      if (filter === "wins") return it.kind === "win";
      if (filter === "bugs") return it.kind === "bug";
      if (filter === "fixed") return it.status === "fixed";
      return true;
    });
  }, [feed, filter]);

  return (
    <div className="max-w-2xl mx-auto">
      {/* ---- header ---- */}
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
        className="pt-12 pb-2"
      >
        <h1 className="font-atb-serif text-3xl sm:text-4xl font-semibold tracking-tight leading-tight">
          BAML is a data-driven language.
        </h1>
        <p className="mt-3 text-atb-ink-2 leading-relaxed text-[15px]">
          Agents write programs in BAML and report on what went well and what
          can be improved.
        </p>
      </motion.div>

      {/* ---- filters ---- */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.7, delay: 0.2, ease: EASE }}
        className="py-3 mt-2 flex items-center gap-2"
      >
        <div className="flex bg-atb-ivory border border-atb-line rounded-full p-0.5">
          {FILTERS.map((f) => (
            <button
              key={f}
              onClick={() => {
                setFilter(f);
                setShown(PAGE);
              }}
              className={`relative px-4 py-1 text-xs font-medium rounded-full transition-colors ${
                filter === f ? "text-atb-cloud" : "text-atb-ink-2 hover:text-atb-ink"
              }`}
            >
              {filter === f && (
                <motion.span
                  layoutId="feed-filter"
                  className="absolute inset-0 bg-atb-ink rounded-full"
                  transition={{ type: "spring", stiffness: 400, damping: 34 }}
                />
              )}
              <span className="relative capitalize">{f}</span>
            </button>
          ))}
        </div>
        <Link
          href="/atb/issues"
          className="ml-auto text-xs text-atb-accent-deep hover:text-atb-accent transition-colors"
        >
          issue tracker →
        </Link>
      </motion.div>

      {/* ---- feed ---- */}
      {!items ? (
        <div className="space-y-3 mt-2">
          {[...Array(5)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <p className="py-16 text-center text-sm text-atb-ink-3 max-w-md mx-auto leading-relaxed">
          {filter === "fixed"
            ? "No verified fixes yet. A bug is marked fixed once the verifier confirms it no longer reproduces on a newer nightly."
            : "nothing here yet"}
        </p>
      ) : (
        <>
          <div className="space-y-3 mt-2">
            {items.slice(0, shown).map((it, i) => (
              <FeedCard key={it.id} item={it} index={i} now={now} />
            ))}
          </div>
          {items.length > shown && (
            <button
              onClick={() => setShown((s) => s + PAGE)}
              className="mt-6 w-full py-3 text-sm text-atb-ink-2 bg-atb-ivory border border-atb-line rounded-2xl hover:border-atb-accent/50 hover:text-atb-ink transition-colors"
            >
              show more ({items.length - shown} left)
            </button>
          )}
        </>
      )}
    </div>
  );
}

// ---- cards ----

const STATUS_CHIP: Record<FeedStatus, { label: string; cls: string }> = {
  reported: { label: "reported", cls: "bg-atb-amber-soft text-atb-amber" },
  fixing: { label: "fix in progress", cls: "bg-atb-slate-soft text-atb-slate" },
  fixed: { label: "fixed", cls: "bg-atb-accent-soft text-atb-accent-deep" },
  rejected: { label: "rejected", cls: "bg-atb-oat text-atb-ink-3" },
};

function avatar(item: FeedItem): { emoji: string; cls: string } {
  if (item.kind === "win")
    return { emoji: "✓", cls: "bg-atb-olive-soft text-atb-olive" };
  if (item.status === "fixed")
    return { emoji: "✓", cls: "bg-atb-accent-soft text-atb-accent-deep" };
  if (item.status === "fixing") return { emoji: "🔧", cls: "bg-atb-slate-soft" };
  return { emoji: "🐛", cls: "bg-atb-amber-soft" };
}

function FeedCard({
  item,
  index,
  now,
}: {
  item: FeedItem;
  index: number;
  now: number;
}) {
  const href =
    item.kind === "bug" ? `/atb/issues/${item.issueId}` : `/atb/runs/${item.runId}`;
  const av = avatar(item);
  const chip = item.status ? STATUS_CHIP[item.status] : null;

  return (
    <motion.div
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{
        duration: 0.5,
        delay: Math.min((index % PAGE) * 0.05, 0.6),
        ease: EASE,
      }}
    >
      <Link href={href} className="block group">
        <article className="bg-atb-ivory/60 border border-atb-line rounded-2xl px-5 py-4 hover:border-atb-accent/50 hover:-translate-y-px transition-all duration-300">
          <div className="flex gap-3.5">
            {/* avatar */}
            <div
              className={`w-10 h-10 rounded-full grid place-items-center text-base shrink-0 ${av.cls}`}
            >
              {av.emoji}
            </div>

            <div className="flex-1 min-w-0">
              {/* header line */}
              <div className="flex items-center gap-2 text-[13px] min-w-0">
                <span className="font-semibold text-atb-ink shrink-0">
                  {item.kind === "win" ? "baml agent" : "issue tracker"}
                </span>
                <span className="text-atb-ink-3 truncate font-atb-mono text-xs">
                  {item.kind === "win"
                    ? item.skillRef
                      ? `@${item.skillRef}`
                      : `via ${item.source ?? "cron"}`
                    : item.issueKind}
                </span>
                <span className="text-atb-ink-3 text-xs shrink-0">
                  · {timeAgo(item.at, now)}
                </span>
                {chip && (
                  <span
                    className={`ml-auto shrink-0 px-2 py-0.5 rounded-full text-[11px] font-medium ${chip.cls}`}
                  >
                    {chip.label}
                  </span>
                )}
              </div>

              {/* body */}
              <p
                className={`mt-1.5 text-[15px] leading-relaxed text-atb-ink ${
                  item.kind === "bug" ? "font-medium" : ""
                }`}
              >
                {item.text}
              </p>
              {item.detail && (
                <p className="mt-1 text-sm text-atb-ink-2 leading-relaxed line-clamp-2">
                  {item.detail}
                </p>
              )}
              {item.kind === "win" && item.taskPrompt && (
                <p className="mt-1.5 text-xs text-atb-ink-3 line-clamp-1">
                  while working on: {item.taskPrompt}
                </p>
              )}

              {/* footer */}
              <div className="mt-2.5 flex flex-wrap items-center gap-2 text-[11px]">
                {item.kind === "bug" ? (
                  <>
                    {item.brokeIn && (
                      <span className="font-atb-mono px-2 py-0.5 rounded-full bg-atb-rust-soft/60 text-atb-rust">
                        broke in {item.brokeIn}
                      </span>
                    )}
                    {item.fixedIn && (
                      <span className="font-atb-mono px-2 py-0.5 rounded-full bg-atb-accent-soft text-atb-accent-deep">
                        fixed in {item.fixedIn}
                      </span>
                    )}
                    {(item.evidenceCount ?? 0) > 0 && (
                      <span className="text-atb-ink-3">
                        {item.evidenceCount} run
                        {item.evidenceCount === 1 ? "" : "s"} hit this
                      </span>
                    )}
                    <span className="ml-auto text-atb-accent-deep opacity-0 group-hover:opacity-100 transition-opacity">
                      repro & transcript →
                    </span>
                  </>
                ) : (
                  <>
                    {item.bamlVersion && (
                      <span className="font-atb-mono text-atb-ink-3">
                        baml {item.bamlVersion}
                      </span>
                    )}
                    <span className="ml-auto text-atb-accent-deep opacity-0 group-hover:opacity-100 transition-opacity">
                      view run →
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>
        </article>
      </Link>
    </motion.div>
  );
}
