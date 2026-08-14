// Server-side access to the changelog feed, read DIRECTLY from Convex (the
// changelogEntries table in the same deployment the /atb dashboard reads).
// Shared by the /changelog page (ISR) and the /api/changelog-feed routes so
// there is exactly one Convex query + sort + shape in the codebase.

const CONVEX_URL = process.env.NEXT_PUBLIC_ATB_CONVEX_URL;

export interface ChangelogEntry {
  authors: string[];
  body: string;
  channel: string | null;
  date: string;
  title: string;
  version: string;
}

interface ChangelogRow {
  version?: string;
  date?: string | null;
  title?: string | null;
  body?: string | null;
  authors?: string[] | null;
  channel?: string | null;
  createdAt?: number | null;
}

async function convexQuery<T>(
  path: string,
  args: object,
  revalidate: number | undefined,
): Promise<T> {
  if (!CONVEX_URL) throw new Error('changelog data source not configured');
  const res = await fetch(`${CONVEX_URL}/api/query`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, args, format: 'json' }),
    // With `revalidate`, the response participates in ISR/data cache; without
    // it (route handlers), always hit Convex live.
    ...(revalidate === undefined
      ? { cache: 'no-store' as const }
      : { next: { revalidate } }),
  });
  if (!res.ok) throw new Error(`convex ${path}: ${res.status}`);
  const body = (await res.json()) as { status: string; value: T };
  if (body.status !== 'success') throw new Error(`convex ${path} failed`);
  return body.value;
}

// All published entries, newest-first by published date then creation time
// (matches the old changelog service contract). Throws on any upstream
// failure — callers decide how to degrade.
export async function fetchChangelogEntries(opts?: {
  revalidate?: number;
}): Promise<ChangelogEntry[]> {
  const rows = await convexQuery<ChangelogRow[]>(
    'changelogEntries:list',
    {
      field: 'status',
      value: 'done',
      index: 'by_status_created',
      limit: 1000,
    },
    opts?.revalidate,
  );
  const sorted = [...rows].sort((a, b) => {
    const d = (b.date ?? '').localeCompare(a.date ?? '');
    if (d !== 0) return d;
    return (b.createdAt ?? 0) - (a.createdAt ?? 0);
  });
  return sorted.map((e) => ({
    authors: e.authors ?? [],
    body: e.body ?? '',
    channel: e.channel ?? null,
    date: e.date ?? '',
    title: e.title ?? '',
    version: e.version ?? '',
  }));
}
