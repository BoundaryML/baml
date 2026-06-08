import type { Metadata } from 'next';
import ChangelogBody from './changelog-body';

// Always render fresh so newly-generated entries show up immediately.
export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Changelog | BAML',
  description:
    'The latest releases of BAML, a typed language for reliable LLM functions.',
};

// The changelog microservice owns the database. Set CHANGELOG_API in the
// environment (fly.ui.toml [env]) to the service base URL. Server-only.
const CHANGELOG_API = process.env.CHANGELOG_API;

interface Entry {
  authors: string[];
  body: string;
  date: string;
  title: string;
  version: string;
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
    const res = await fetch(`${CHANGELOG_API}/entries`, {
      cache: 'no-store',
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) return [];
    const data = (await res.json()) as { entries: Entry[] };
    return data.entries ?? [];
  } catch {
    return [];
  }
}

export default async function ChangelogPage() {
  const entries = await loadEntries();

  return (
    <main className="mx-auto max-w-[720px] px-6 pt-16 pb-32">
      <header className="mb-14">
        <h1 className="m-0 text-[clamp(40px,6vw,64px)] font-semibold leading-none tracking-[-0.02em]">
          Changelog
        </h1>
        <p className="mt-5 max-w-[460px] text-lg text-[#6b6456]">
          The latest releases of BAML, a typed language for reliable LLM
          functions.
        </p>
      </header>

      {entries.length === 0 ? (
        <p className="text-[#6b6456]">No entries yet.</p>
      ) : (
        <ol className="relative m-0 list-none p-0">
          {/* continuous vertical rail running through every marker */}
          <div
            aria-hidden
            className="absolute top-2 bottom-2 left-[140px] w-px bg-[#D9D3C4]"
          />

          {entries.map((e) => (
            <li
              key={e.version}
              id={e.version}
              className="relative grid grid-cols-[120px_40px_1fr] items-start py-10 scroll-mt-6"
            >
              <time
                dateTime={e.date}
                className="col-start-1 pt-2 text-right text-xs font-medium whitespace-nowrap text-[#6b6456]"
              >
                {formatDate(e.date)}
              </time>

              <span
                aria-hidden
                className="relative z-[1] col-start-2 mt-[7px] flex size-3 items-center justify-center justify-self-center rounded-full border border-[#D9D3C4] bg-[#FBF7ED]"
              >
                <span className="size-1 rounded-full bg-[#1a1a1a]" />
              </span>

              <div className="col-start-3 min-w-0">
                <h2 className="mb-4 text-2xl font-semibold leading-[1.2] tracking-[-0.01em]">
                  <a href={`#${e.version}`} className="text-inherit">
                    {e.title}
                  </a>
                </h2>

                <ChangelogBody>{e.body}</ChangelogBody>

                {e.authors.length > 0 && (
                  <p className="mt-7 text-xs text-[#6b6456]">
                    By {e.authors.join(', ')}
                  </p>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}
    </main>
  );
}
