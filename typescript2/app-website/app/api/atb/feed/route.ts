import { NextResponse } from "next/server";
import type { Feed, FeedItem } from "@/app/atb/_lib/feed";
import { feedStatus } from "@/app/atb/_lib/feed";
import type { Build, Issue, Task, Trophy } from "@/app/atb/_lib/types";

// Builds the front-page feed: every whatWentWell observation from run
// self-reports becomes a win card, every tracked issue becomes a bug card
// with its lifecycle state. Fetched server side (the full trophies list is
// ~5 MB) and cached for 30s.

const CONVEX_URL = process.env.NEXT_PUBLIC_ATB_CONVEX_URL;

async function convexQuery<T>(path: string, args: object): Promise<T> {
  const res = await fetch(`${CONVEX_URL}/api/query`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path, args, format: "json" }),
    cache: "no-store",
  });
  if (!res.ok) throw new Error(`convex ${path}: ${res.status}`);
  const body = (await res.json()) as { status: string; value: T };
  if (body.status !== "success") throw new Error(`convex ${path} failed`);
  return body.value;
}

const refLabel = (ref?: string | null) =>
  ref ? ref.replace(/^baml-language-/, "") : null;

/** First readable sentence-ish chunk of an issue's markdown description. */
function snippet(md: string, max = 220): string {
  const text = md
    .replace(/^#+\s.*$/gm, "") // headings
    .replace(/```[\s\S]*?```/g, "") // code fences
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\n{2,}/g, "\n")
    .trim()
    .split("\n")[0]
    ?.trim();
  if (!text) return "";
  return text.length > max ? `${text.slice(0, max).trimEnd()}…` : text;
}

const trunc = (s: string | undefined | null, n: number) =>
  s == null ? null : s.length > n ? `${s.slice(0, n).trimEnd()}…` : s;

async function buildFeed(): Promise<Feed> {
  const [trophies, tasks, issues, builds] = await Promise.all([
    convexQuery<Trophy[]>("trophies:list", { limit: 500 }),
    convexQuery<Task[]>("tasks:list", { limit: 500 }),
    convexQuery<Issue[]>("issues:list", { limit: 500 }),
    convexQuery<Build[]>("bamlBuilds:list", { limit: 50 }),
  ]);

  const taskById = new Map(tasks.map((t) => [t._id, t]));
  const trophyById = new Map(trophies.map((t) => [t._id, t]));
  const labelBySha = new Map(builds.map((b) => [b.sha, refLabel(b.ref)]));
  const versionLabel = (sha?: string | null) =>
    sha === "coldstart"
      ? "cold start"
      : (sha && (labelBySha.get(sha) ?? sha.slice(0, 8))) || null;

  const items: FeedItem[] = [];

  // ---- wins: each self-reported "what went well" line ----
  let wins = 0;
  for (const tr of trophies) {
    if (tr.isCohortReport) continue;
    const task = taskById.get(tr.taskId);
    const well = tr.whatWentWell ?? [];
    well.forEach((text, i) => {
      wins++;
      items.push({
        id: `${tr._id}-w${i}`,
        kind: "win",
        text,
        at: tr.createdAt - i, // keep a stable order within one run
        skillRef: task?.skillRef ?? null,
        source: task?.source ?? null,
        taskPrompt: trunc(task?.prompt, 120),
        bamlVersion: versionLabel(tr.bamlVersion),
        runId: tr._id,
      });
    });
  }

  // ---- bugs: one card per tracked issue ----
  let bugs = 0;
  let fixed = 0;
  for (const issue of issues) {
    const status = feedStatus(issue);
    if (!status) continue;
    bugs++;
    if (status === "fixed") fixed++;
    // the run that first hit it tells us which version it broke in
    const firstEvidence = (issue.evidence ?? []).find((e) => e.trophyId);
    const evidenceTrophy = firstEvidence
      ? trophyById.get(firstEvidence.trophyId!)
      : undefined;
    const extra = issue as Issue & {
      brokeIn?: string | null;
      fixedIn?: string | null;
    };
    items.push({
      id: issue._id,
      kind: "bug",
      text: issue.title,
      detail: snippet(issue.description),
      at: issue.lastSeenAt,
      bamlVersion: versionLabel(evidenceTrophy?.bamlVersion),
      issueId: issue._id,
      status,
      issueKind: issue.kind,
      category: issue.category ?? null,
      evidenceCount: (issue.evidence ?? []).length,
      brokeIn: extra.brokeIn ?? versionLabel(evidenceTrophy?.bamlVersion),
      fixedIn: extra.fixedIn ?? null,
    });
  }

  items.sort((a, b) => b.at - a.at);

  return {
    generatedAt: Date.now(),
    counts: {
      wins,
      bugs,
      fixed,
      runs: trophies.filter((t) => !t.isCohortReport).length,
    },
    items: items.slice(0, 600),
  };
}

const TTL_MS = 30_000;
let cache: { at: number; feed: Feed } | null = null;
let inflight: Promise<Feed> | null = null;

function refresh(): Promise<Feed> {
  if (!inflight) {
    inflight = buildFeed()
      .then((feed) => {
        cache = { at: Date.now(), feed };
        return feed;
      })
      .finally(() => {
        inflight = null;
      });
  }
  return inflight;
}

export async function GET() {
  if (!CONVEX_URL) {
    return new NextResponse("atb data source not configured", { status: 503 });
  }
  let feed: Feed;
  if (cache && Date.now() - cache.at < TTL_MS) {
    feed = cache.feed;
  } else if (cache) {
    void refresh().catch(() => {});
    feed = cache.feed;
  } else {
    feed = await refresh();
  }
  return NextResponse.json(feed, {
    headers: {
      "Cache-Control": "public, s-maxage=30, stale-while-revalidate=300",
    },
  });
}
