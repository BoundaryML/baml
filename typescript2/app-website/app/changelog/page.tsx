import type { Metadata } from 'next';
import rehypeShiki from '@shikijs/rehype';
import rehypeStringify from 'rehype-stringify';
import remarkGfm from 'remark-gfm';
import remarkParse from 'remark-parse';
import remarkRehype from 'remark-rehype';
import { bundledLanguages } from 'shiki';
import { unified } from 'unified';

import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import {
  bamlJinjaTextmate,
  bamlTextmate,
} from '@/lib/mdx/shiki-grammars';

// Always render fresh so newly-generated entries show up immediately.
export const dynamic = 'force-dynamic';

const baseUrl =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'https://boundaryml.com');

// The microservice owns the database. Set CHANGELOG_API in the environment
// (e.g. the Vercel project env) to the service base URL. Server-only — it is
// read in this server component and never exposed to the client.
const CHANGELOG_API = process.env.CHANGELOG_API;

export const metadata: Metadata = {
  alternates: { canonical: `${baseUrl}/changelog` },
  description:
    'The latest releases of BAML, a typed language for reliable LLM functions.',
  openGraph: {
    description:
      'The latest releases of BAML, a typed language for reliable LLM functions.',
    siteName: 'BAML',
    title: 'BAML Changelog',
    type: 'website',
    url: `${baseUrl}/changelog`,
  },
  title: 'Changelog | BAML',
};

interface Entry {
  authors: string[];
  body: string;
  date: string;
  title: string;
  version: string;
}

// One pipeline reused across all entries. rehype-shiki is ASYNC (lazy-loads
// grammars on first use), which is why we cannot route this through
// react-markdown (which calls `runSync`).
const md = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkRehype)
  .use(rehypeShiki, {
    defaultColor: 'dark',
    langs: [
      ...Object.values(bundledLanguages),
      bamlTextmate,
      bamlJinjaTextmate,
    ],
    themes: { dark: 'github-dark', light: 'github-light' },
  })
  .use(rehypeStringify);

async function renderBody(body: string): Promise<string> {
  const file = await md.process(body);
  return String(file);
}

function formatDate(iso: string): string {
  const [y, m, d] = iso.split('-').map(Number);
  const date = new Date(Date.UTC(y, m - 1, d));
  return date.toLocaleDateString('en-US', {
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
    year: 'numeric',
  });
}

async function loadEntries(): Promise<Entry[]> {
  if (!CHANGELOG_API) return [];
  try {
    const res = await fetch(`${CHANGELOG_API}/entries`, { cache: 'no-store' });
    if (!res.ok) return [];
    const data = (await res.json()) as { entries: Entry[] };
    return data.entries ?? [];
  } catch {
    return [];
  }
}

export default async function ChangelogPage() {
  const entries = await loadEntries();
  // Pre-render all bodies in parallel so each <article> has its HTML ready.
  const rendered = await Promise.all(
    entries.map(async (e) => ({ ...e, html: await renderBody(e.body) })),
  );

  return (
    <>
      <Navbar />
      <main className="mx-auto max-w-4xl px-6 pt-24 pb-32">
        <header className="mb-16">
          <h1 className="text-6xl font-semibold leading-none tracking-tight">
            Changelog
          </h1>
          <p className="text-muted-foreground mt-5 max-w-xl text-lg">
            The latest releases of BAML, a typed language for reliable LLM
            functions.
          </p>
        </header>

        {rendered.length === 0 ? (
          <p className="text-muted-foreground">No entries yet.</p>
        ) : (
          <ol className="relative">
            {/* the continuous vertical rail running through every circle */}
            <div
              aria-hidden
              className="bg-border absolute top-2 bottom-2 left-[140px] w-px"
            />

            {rendered.map((e) => (
              <li
                key={e.version}
                id={e.version}
                className="relative grid grid-cols-[120px_40px_1fr] items-start py-10 first:pt-0 last:pb-0 scroll-mt-24"
              >
                <time
                  dateTime={e.date}
                  className="text-muted-foreground col-start-1 pt-2 text-right text-xs font-medium whitespace-nowrap"
                >
                  {formatDate(e.date)}
                </time>

                <span
                  aria-hidden
                  className="border-border bg-background relative z-10 col-start-2 mt-[7px] flex h-3 w-3 items-center justify-center justify-self-center rounded-full border"
                >
                  <span className="bg-foreground h-1 w-1 rounded-full" />
                </span>

                <div className="col-start-3 min-w-0">
                  <h2 className="text-foreground mb-4 text-2xl font-semibold leading-tight tracking-tight">
                    <a href={`#${e.version}`} className="hover:underline">
                      {e.title}
                    </a>
                  </h2>

                  <div
                    className="prose prose-neutral dark:prose-invert max-w-none prose-p:my-3 prose-li:my-1.5 prose-headings:font-semibold prose-h3:text-base prose-h3:mt-5 prose-h3:mb-2 prose-code:rounded prose-code:bg-muted prose-code:px-1.5 prose-code:py-0.5 prose-code:font-medium prose-code:text-foreground prose-code:before:content-none prose-code:after:content-none prose-pre:rounded-lg prose-pre:border prose-pre:border-zinc-800 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_pre_code]:rounded-none [&_pre_code]:font-normal [&_pre_code]:text-current [&_pre]:overflow-x-auto"
                    // eslint-disable-next-line react/no-danger
                    dangerouslySetInnerHTML={{ __html: e.html }}
                  />

                  {e.authors.length > 0 && (
                    <p className="text-muted-foreground mt-7 text-xs">
                      By {e.authors.join(', ')}
                    </p>
                  )}
                </div>
              </li>
            ))}
          </ol>
        )}
      </main>
      <FooterSection />
    </>
  );
}
