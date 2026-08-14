import { NextResponse } from 'next/server';
import { fetchChangelogEntries } from '@/app/changelog/feed';

// Single published entry by version, for the /changelog article view (the
// list payload carries no bodies). Published bodies are effectively
// immutable, so cache aggressively at the CDN.

export async function GET(
  _req: Request,
  { params }: { params: Promise<{ version: string }> },
) {
  const { version } = await params;
  const wanted = decodeURIComponent(version);
  try {
    const entries = await fetchChangelogEntries();
    const entry = entries.find((e) => e.version === wanted);
    if (!entry) {
      return new NextResponse('no such release', { status: 404 });
    }
    return NextResponse.json(entry, {
      headers: {
        'Cache-Control': 'public, s-maxage=300, stale-while-revalidate=86400',
      },
    });
  } catch {
    return new NextResponse('changelog data source unavailable', {
      headers: { 'Cache-Control': 'no-store' },
      status: 503,
    });
  }
}
