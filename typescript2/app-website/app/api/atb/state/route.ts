import { NextResponse } from "next/server";
import type {
  AtbState,
  Build,
  Cohort,
  Issue,
  SlimIssue,
  SlimTask,
  SlimTrophy,
  Task,
  Trophy,
  Worker,
} from "@/app/atb/_lib/types";

// The Convex deployment's generic list queries return full documents. A
// full trophies:list is ~5 MB (turn logs, file contents, reports), which
// made first paint take seconds over the websocket. This route fetches the
// lists server side, strips them to what list views render, and caches the
// slim snapshot for a few seconds. Detail pages still subscribe to full
// docs by id.

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

const trunc = (s: string | undefined | null, n: number) =>
  s == null ? s : s.length > n ? s.slice(0, n) : s;

function slimTask(t: Task): SlimTask {
  return {
    _id: t._id,
    source: t.source,
    prompt: trunc(t.prompt, 280) ?? "",
    status: t.status,
    createdAt: t.createdAt,
    updatedAt: t.updatedAt,
    claimedBy: t.claimedBy ?? null,
    claimedAt: t.claimedAt ?? null,
    cohortId: t.cohortId ?? null,
    skillRef: t.skillRef ?? null,
    skillStorageId: t.skillStorageId ?? null,
    bamlVersion: t.bamlVersion ?? null,
  };
}

function slimTrophy(t: Trophy): SlimTrophy {
  return {
    _id: t._id,
    taskId: t.taskId,
    outcome: t.outcome,
    status: t.status,
    metrics: t.metrics,
    findingsCount: t.findings?.length ?? 0,
    createdAt: t.createdAt,
    updatedAt: t.updatedAt,
    claimedBy: t.claimedBy ?? null,
    claimedAt: t.claimedAt ?? null,
    cohortId: t.cohortId ?? null,
    isCohortReport: t.isCohortReport ?? false,
    bamlVersion: t.bamlVersion ?? null,
  };
}

function slimIssue(i: Issue): SlimIssue {
  return {
    _id: i._id,
    kind: i.kind,
    category: i.category ?? null,
    title: i.title,
    status: i.status,
    fixSlackTs: i.fixSlackTs ?? null,
    notionSyncStatus: i.notionSyncStatus,
    linearSyncStatus: i.linearSyncStatus,
    evidenceCount: i.evidence?.length ?? 0,
    evidence: (i.evidence ?? []).map((e) => ({
      trophyId: e.trophyId,
      call_index: e.call_index ?? null,
    })),
    firstSeenAt: i.firstSeenAt,
    lastSeenAt: i.lastSeenAt,
    createdAt: i.createdAt,
  };
}

async function buildState(): Promise<AtbState> {
  const [tasks, trophies, issues, cohorts, builds, workers] =
    await Promise.all([
      convexQuery<Task[]>("tasks:list", { limit: 500 }),
      convexQuery<Trophy[]>("trophies:list", { limit: 500 }),
      convexQuery<Issue[]>("issues:list", { limit: 500 }),
      convexQuery<Cohort[]>("cohorts:list", { limit: 100 }),
      convexQuery<Build[]>("bamlBuilds:list", { limit: 50 }),
      convexQuery<Worker[]>("workers:list", { limit: 100 }),
    ]);
  const newestFirst = <T extends { createdAt: number }>(rows: T[]) =>
    [...rows].sort((a, b) => b.createdAt - a.createdAt);
  return {
    generatedAt: Date.now(),
    tasks: newestFirst(tasks.map(slimTask)),
    trophies: newestFirst(trophies.map(slimTrophy)),
    issues: newestFirst(issues.map(slimIssue)),
    cohorts: newestFirst(cohorts),
    builds: newestFirst(builds.map(({ buildLogTail: _drop, ...b }) => b as Build)),
    workers,
  };
}

// Module-level cache with stale-while-revalidate: a fresh snapshot is served
// for TTL_MS; after that the stale one is returned immediately while one
// background refresh runs. Past MAX_STALE_MS the snapshot is no longer trusted
// — a request blocks on a fresh build and a persistent failure surfaces as a
// 503/stale-flagged response rather than silently freezing a stale snapshot
// forever (which is what happened when the Convex URL was misconfigured).
const TTL_MS = 10_000;
const MAX_STALE_MS = 60_000;
let cache: { at: number; state: AtbState } | null = null;
let inflight: Promise<AtbState> | null = null;

function refresh(): Promise<AtbState> {
  if (!inflight) {
    inflight = buildState()
      .then((state) => {
        cache = { at: Date.now(), state };
        return state;
      })
      .catch((e) => {
        // Surface the failure so a frozen dashboard is visible in logs.
        console.error("atb state refresh failed", e);
        throw e;
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
  const age = cache ? Date.now() - cache.at : Infinity;
  const fresh = {
    "Cache-Control": "public, s-maxage=10, stale-while-revalidate=120",
  };
  if (cache && age < TTL_MS) {
    return NextResponse.json(cache.state, { headers: fresh });
  }
  if (cache && age < MAX_STALE_MS) {
    void refresh().catch(() => {}); // serve stale while revalidating
    return NextResponse.json(cache.state, { headers: fresh });
  }
  // No cache, or the cache is too old to trust: block on a fresh build and let
  // a persistent failure show instead of serving an indefinitely stale snapshot.
  try {
    return NextResponse.json(await refresh(), { headers: fresh });
  } catch {
    if (cache) {
      return NextResponse.json(cache.state, {
        headers: { "Cache-Control": "no-store", "X-Atb-Stale": "true" },
      });
    }
    return new NextResponse("atb data temporarily unavailable", { status: 503 });
  }
}
