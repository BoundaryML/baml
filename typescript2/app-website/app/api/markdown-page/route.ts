import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { type NextRequest, NextResponse } from 'next/server';
import {
  isMarkdownPageSlug,
  type MarkdownPageSlug,
  markdownCanonicalPaths,
} from '@/lib/markdown-pages';

export const dynamic = 'force-dynamic';

const MARKDOWN_FILES: Record<MarkdownPageSlug, string> = {
  explore: 'explore.md',
  index: 'index.md',
  pricing: 'pricing.md',
  quickstart: 'quickstart.md',
};

async function getCheckedInMarkdown(slug: MarkdownPageSlug) {
  return readFile(
    path.join(process.cwd(), 'content', MARKDOWN_FILES[slug]),
    'utf8',
  );
}

export async function GET(request: NextRequest) {
  const slug = request.headers.get('x-baml-markdown-page') ?? '';
  if (!isMarkdownPageSlug(slug)) {
    return new NextResponse('Not found\n', { status: 404 });
  }

  let markdown: string;
  try {
    markdown = await getCheckedInMarkdown(slug);
  } catch {
    return new NextResponse(
      'Unable to render the complete page as Markdown.\n',
      {
        headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
        status: 502,
      },
    );
  }
  const canonical = new URL(
    markdownCanonicalPaths[slug],
    request.nextUrl.origin,
  );

  return new NextResponse(markdown, {
    headers: {
      'Cache-Control': 'public, s-maxage=3600, stale-while-revalidate=86400',
      'Content-Type': 'text/markdown; charset=utf-8',
      Link: `<${canonical}>; rel="canonical"`,
      'X-Content-Type-Options': 'nosniff',
    },
  });
}
