import { NextResponse } from "next/server";

// The public changelog feed, read DIRECTLY from Convex (the changelogEntries
// table in the same deployment the /atb dashboard reads) instead of proxying to
// the bammy changelog service. This removes the dependency on a running backend
// service: as long as the changelog worker keeps writing entries to Convex, the
// public page renders. Shape matches the old service contract: { entries: [...] }
// with one object per published (status=done) entry, newest-first.

const CONVEX_URL = process.env.NEXT_PUBLIC_ATB_CONVEX_URL;

// The fields the website's changelog renderer consumes.
const ENTRY_FIELDS = [
  "version",
  "date",
  "title",
  "body",
  "authors",
  "channel",
] as const;

interface ChangelogRow {
  version?: string;
  date?: string | null;
  title?: string | null;
  body?: string | null;
  authors?: string[] | null;
  channel?: string | null;
  createdAt?: number | null;
}

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

export async function GET() {
  if (!CONVEX_URL) {
    return new NextResponse("changelog data source not configured", {
      status: 503,
    });
  }
  try {
    const rows = await convexQuery<ChangelogRow[]>("changelogEntries:list", {
      field: "status",
      value: "done",
      index: "by_status_created",
      limit: 1000,
    });
    // Newest-first by published date, then creation time (matches the old service).
    const sorted = [...rows].sort((a, b) => {
      const d = (b.date ?? "").localeCompare(a.date ?? "");
      if (d !== 0) return d;
      return (b.createdAt ?? 0) - (a.createdAt ?? 0);
    });
    const entries = sorted.map((e) =>
      Object.fromEntries(ENTRY_FIELDS.map((k) => [k, e[k] ?? null])),
    );
    return NextResponse.json(
      { entries },
      {
        headers: {
          "Cache-Control": "public, s-maxage=60, stale-while-revalidate=300",
        },
      },
    );
  } catch {
    // Never 500 the changelog: degrade to an empty feed (the page shows
    // "No entries yet." rather than an error).
    return NextResponse.json(
      { entries: [] },
      { headers: { "Cache-Control": "no-store" } },
    );
  }
}
