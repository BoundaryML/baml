import { NextResponse } from 'next/server';
import type { Feed, FeedItem } from '@/app/atb/_lib/feed';
import { feedStatus } from '@/app/atb/_lib/feed';
import {
  ATB_TROPHIES_QUERY_LIMIT,
  type Build,
  type Issue,
  type Task,
  type Trophy,
} from '@/app/atb/_lib/types';

// Builds the front-page feed: every whatWentWell observation from run
// self-reports becomes a win card, every tracked issue becomes a bug card
// with its lifecycle state. Fetched server side (the full trophies list is
// ~5 MB) and cached for 30s.

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

const refLabel = (ref?: string | null) =>
  ref ? ref.replace(/^baml-language-/, '') : null;

/** First readable sentence-ish chunk of an issue's markdown description. */
function snippet(md: string, max = 220): string {
  const text = md
    .replace(/^#+\s.*$/gm, '') // headings
    .replace(/```[\s\S]*?```/g, '') // code fences
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\n{2,}/g, '\n')
    .trim()
    .split('\n')[0]
    ?.trim();
  if (!text) return '';
  return text.length > max ? `${text.slice(0, max).trimEnd()}…` : text;
}

const trunc = (s: string | undefined | null, n: number) =>
  s == null ? null : s.length > n ? `${s.slice(0, n).trimEnd()}…` : s;

async function buildFeed(): Promise<Feed> {
  const [trophies, tasks, issues, builds] = await Promise.all([
    convexQuery<Trophy[]>('trophies:list', { limit: ATB_TROPHIES_QUERY_LIMIT }),
    convexQuery<Task[]>('tasks:list', { limit: 500 }),
    convexQuery<Issue[]>('issues:list', { limit: 500 }),
    convexQuery<Build[]>('bamlBuilds:list', { limit: 50 }),
  ]);

  const taskById = new Map(tasks.map((t) => [t._id, t]));
  const trophyById = new Map(trophies.map((t) => [t._id, t]));
  const labelBySha = new Map(builds.map((b) => [b.sha, refLabel(b.ref)]));
  const versionLabel = (sha?: string | null) =>
    sha === 'coldstart'
      ? 'cold start'
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
        at: tr.createdAt - i, // keep a stable order within one run
        bamlVersion: versionLabel(tr.bamlVersion),
        id: `${tr._id}-w${i}`,
        kind: 'win',
        runId: tr._id,
        skillRef: task?.skillRef ?? null,
        source: task?.source ?? null,
        taskPrompt: trunc(task?.prompt, 120),
        text,
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
    if (status === 'fixed') fixed++;
    // the run that first hit it tells us which version it broke in
    const firstEvidence = (issue.evidence ?? []).find((e) => e.trophyId);
    const evidenceTrophy = firstEvidence?.trophyId
      ? trophyById.get(firstEvidence.trophyId)
      : undefined;
    const extra = issue as Issue & {
      brokeIn?: string | null;
      fixedIn?: string | null;
    };
    items.push({
      at: issue.lastSeenAt,
      bamlVersion: versionLabel(evidenceTrophy?.bamlVersion),
      brokeIn: extra.brokeIn ?? versionLabel(evidenceTrophy?.bamlVersion),
      category: issue.category ?? null,
      detail: snippet(issue.description),
      evidenceCount: (issue.evidence ?? []).length,
      fixedIn: extra.fixedIn ?? null,
      id: issue._id,
      issueId: issue._id,
      issueKind: issue.kind,
      kind: 'bug',
      status,
      text: issue.title,
    });
  }

  items.sort((a, b) => b.at - a.at);

  return {
    counts: {
      bugs,
      fixed,
      runs: trophies.filter((t) => !t.isCohortReport).length,
      wins,
    },
    generatedAt: Date.now(),
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
    return new NextResponse('atb data source not configured', { status: 503 });
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
      'Cache-Control': 'public, s-maxage=30, stale-while-revalidate=300',
    },
  });
}
