import { NextResponse } from 'next/server';
import { fetchChangelogEntries } from '@/app/changelog/feed';

// The public changelog feed. The /changelog page no longer depends on this
// route (it server-renders via ISR from the same shared feed module); this
// stays up as the external contract: { entries: [...] } with one object per
// published entry, newest-first.

const ENTRY_FIELDS = [
  'version',
  'date',
  'title',
  'body',
  'authors',
  'channel',
] as const;

export async function GET() {
  try {
    const entries = (await fetchChangelogEntries()).map((e) =>
      Object.fromEntries(ENTRY_FIELDS.map((k) => [k, e[k] ?? null])),
    );
    return NextResponse.json(
      { entries },
      {
        headers: {
          'Cache-Control': 'public, s-maxage=60, stale-while-revalidate=300',
        },
      },
    );
  } catch {
    // Never 500 the changelog: degrade to an empty feed.
    return NextResponse.json(
      { entries: [] },
      { headers: { 'Cache-Control': 'no-store' } },
    );
  }
}
