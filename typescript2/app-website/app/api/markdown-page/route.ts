import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { type NextRequest, NextResponse } from 'next/server';
import {
  isMarkdownPageSlug,
  markdownCanonicalPaths,
  type MarkdownPageSlug,
} from '@/lib/markdown-pages';

export const dynamic = 'force-dynamic';

interface ChangelogEntry {
  authors?: string[] | null;
  body?: string | null;
  channel?: string | null;
  date?: string | null;
  title?: string | null;
  version?: string | null;
}

const MARKDOWN_FILES: Record<MarkdownPageSlug, string> = {
  index: 'index.md',
  quickstart: 'quickstart.md',
  explore: 'explore.md',
  pricing: 'pricing.md',
  changelog: 'changelog.md',
};

async function getCheckedInMarkdown(slug: MarkdownPageSlug) {
  return readFile(
    path.join(process.cwd(), 'content', MARKDOWN_FILES[slug]),
    'utf8',
  );
}

async function getChangelogMarkdown(request: NextRequest, base: string) {

  try {
    const response = await fetch(
      new URL('/api/changelog-feed/entries', request.nextUrl.origin),
      { cache: 'no-store' },
    );
    if (!response.ok) return base;

    const data = (await response.json()) as { entries?: ChangelogEntry[] };
    const entries = data.entries ?? [];
    if (entries.length === 0) return base;

    const rendered = entries.map((entry) => {
      const heading = entry.title || entry.version || 'BAML release';
      const metadata = [entry.version, entry.channel, entry.date]
        .filter(Boolean)
        .join(' · ');
      const authors = entry.authors?.length
        ? `\n\nAuthors: ${entry.authors.join(', ')}`
        : '';
      return `## ${heading}\n\n${metadata}${entry.body ? `\n\n${entry.body}` : ''}${authors}`;
    });

    return `${base}\n${rendered.join('\n\n')}\n`;
  } catch {
    return base;
  }
}

export async function GET(request: NextRequest) {
  const slug = request.headers.get('x-baml-markdown-page') ?? '';
  if (!isMarkdownPageSlug(slug)) {
    return new NextResponse('Not found\n', { status: 404 });
  }

  let markdown: string;
  try {
    const rendered = await getCheckedInMarkdown(slug);
    markdown =
      slug === 'changelog'
        ? await getChangelogMarkdown(request, rendered)
        : rendered;
  } catch {
    return new NextResponse('Unable to render the complete page as Markdown.\n', {
      headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
      status: 502,
    });
  }
  const canonical = new URL(markdownCanonicalPaths[slug], request.nextUrl.origin);

  return new NextResponse(markdown, {
    headers: {
      'Cache-Control':
        slug === 'changelog'
          ? 'public, s-maxage=60, stale-while-revalidate=300'
          : 'public, s-maxage=3600, stale-while-revalidate=86400',
      'Content-Type': 'text/markdown; charset=utf-8',
      Link: `<${canonical}>; rel="canonical"`,
      'X-Content-Type-Options': 'nosniff',
    },
  });
}
