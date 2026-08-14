import { NextResponse } from 'next/server';
import {
  ATB_TROPHIES_QUERY_LIMIT,
  type AtbState,
  type Build,
  type Cohort,
  type Issue,
  type SlimIssue,
  type SlimTask,
  type SlimTrophy,
  type Task,
  type Trophy,
  type Worker,
} from '@/app/atb/_lib/types';

// The Convex deployment's generic list queries return full documents. A
// full trophies:list is ~5 MB (turn logs, file contents, reports), which
// made first paint take seconds over the websocket. This route fetches the
// lists server side, strips them to what list views render, and caches the
// slim snapshot for a few seconds. Detail pages still subscribe to full
// docs by id.

const CONVEX_URL = process.env.NEXT_PUBLIC_ATB_CONVEX_URL;

async function convexQuery<T>(path: string, args: object): Promise<T> {
  const res = await fetch(`${CONVEX_URL}/api/query`, {
    body: JSON.stringify({ args, format: 'json', path }),
    cache: 'no-store',
    headers: { 'Content-Type': 'application/json' },
    method: 'POST',
  });
  if (!res.ok) throw new Error(`convex ${path}: ${res.status}`);
  const body = (await res.json()) as { status: string; value: T };
  if (body.status !== 'success') throw new Error(`convex ${path} failed`);
  return body.value;
}

const trunc = (s: string | undefined | null, n: number) =>
  s == null ? s : s.length > n ? s.slice(0, n) : s;

function slimTask(t: Task): SlimTask {
  return {
    _id: t._id,
    bamlVersion: t.bamlVersion ?? null,
    claimedAt: t.claimedAt ?? null,
    claimedBy: t.claimedBy ?? null,
    cohortId: t.cohortId ?? null,
    createdAt: t.createdAt,
    prompt: trunc(t.prompt, 280) ?? '',
    skillRef: t.skillRef ?? null,
    skillStorageId: t.skillStorageId ?? null,
    source: t.source,
    status: t.status,
    updatedAt: t.updatedAt,
  };
}

function slimTrophy(t: Trophy): SlimTrophy {
  return {
    _id: t._id,
    bamlVersion: t.bamlVersion ?? null,
    claimedAt: t.claimedAt ?? null,
    claimedBy: t.claimedBy ?? null,
    cohortId: t.cohortId ?? null,
    createdAt: t.createdAt,
    findingsCount: t.findings?.length ?? 0,
    isCohortReport: t.isCohortReport ?? false,
    metrics: t.metrics,
    outcome: t.outcome,
    status: t.status,
    taskId: t.taskId,
    updatedAt: t.updatedAt,
  };
}

function slimIssue(i: Issue): SlimIssue {
  return {
    _id: i._id,
    bamlVersion: i.bamlVersion ?? null,
    brokeIn: i.brokeIn ?? null,
    category: i.category ?? null,
    createdAt: i.createdAt,
    evidence: (i.evidence ?? []).map((e) => ({
      call_index: e.call_index ?? null,
      trophyId: e.trophyId,
    })),
    evidenceCount: i.evidence?.length ?? 0,
    firstSeenAt: i.firstSeenAt,
    fixedIn: i.fixedIn ?? null,
    fixSlackTs: i.fixSlackTs ?? null,
    kind: i.kind,
    lastSeenAt: i.lastSeenAt,
    linearSyncStatus: i.linearSyncStatus,
    notionSyncStatus: i.notionSyncStatus,
    skillUsed: i.skillUsed ?? null,
    skillVersion: i.skillVersion ?? null,
    status: i.status,
    title: i.title,
    verifiedAt: i.verifiedAt ?? null,
  };
}

async function buildState(): Promise<AtbState> {
  const [tasks, trophies, issues, cohorts, builds, workers] = await Promise.all(
    [
      convexQuery<Task[]>('tasks:list', { limit: 500 }),
      convexQuery<Trophy[]>('trophies:list', {
        limit: ATB_TROPHIES_QUERY_LIMIT,
      }),
      convexQuery<Issue[]>('issues:list', { limit: 500 }),
      convexQuery<Cohort[]>('cohorts:list', { limit: 100 }),
      convexQuery<Build[]>('bamlBuilds:list', { limit: 50 }),
      convexQuery<Worker[]>('workers:list', { limit: 100 }),
    ],
  );
  const newestFirst = <T extends { createdAt: number }>(rows: T[]) =>
    [...rows].sort((a, b) => b.createdAt - a.createdAt);
  return {
    builds: newestFirst(
      builds.map(({ buildLogTail: _drop, ...b }) => b as Build),
    ),
    cohorts: newestFirst(cohorts),
    generatedAt: Date.now(),
    issues: newestFirst(issues.map(slimIssue)),
    tasks: newestFirst(tasks.map(slimTask)),
    trophies: newestFirst(trophies.map(slimTrophy)),
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
        console.error('atb state refresh failed', e);
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
    return new NextResponse('atb data source not configured', { status: 503 });
  }
  const age = cache ? Date.now() - cache.at : Number.POSITIVE_INFINITY;
  const fresh = {
    'Cache-Control': 'public, s-maxage=10, stale-while-revalidate=120',
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
        headers: { 'Cache-Control': 'no-store', 'X-Atb-Stale': 'true' },
      });
    }
    return new NextResponse('atb data temporarily unavailable', {
      status: 503,
    });
  }
}
