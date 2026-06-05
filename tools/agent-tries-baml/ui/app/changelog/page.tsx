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
    <main
      style={{
        margin: '0 auto',
        maxWidth: 720,
        padding: '64px 24px 128px',
      }}
    >
      <header style={{ marginBottom: 56 }}>
        <h1
          style={{
            fontSize: 'clamp(40px, 6vw, 64px)',
            fontWeight: 600,
            letterSpacing: '-0.02em',
            lineHeight: 1,
            margin: 0,
          }}
        >
          Changelog
        </h1>
        <p
          style={{
            color: '#6b6456',
            fontSize: 18,
            margin: '20px 0 0',
            maxWidth: 460,
          }}
        >
          The latest releases of BAML, a typed language for reliable LLM
          functions.
        </p>
      </header>

      {entries.length === 0 ? (
        <p style={{ color: '#6b6456' }}>No entries yet.</p>
      ) : (
        <ol
          style={{
            listStyle: 'none',
            margin: 0,
            padding: 0,
            position: 'relative',
          }}
        >
          {/* continuous vertical rail running through every marker */}
          <div
            aria-hidden
            style={{
              background: '#D9D3C4',
              bottom: 8,
              left: 140,
              position: 'absolute',
              top: 8,
              width: 1,
            }}
          />

          {entries.map((e) => (
            <li
              key={e.version}
              id={e.version}
              style={{
                alignItems: 'start',
                display: 'grid',
                gridTemplateColumns: '120px 40px 1fr',
                padding: '40px 0',
                position: 'relative',
                scrollMarginTop: 24,
              }}
            >
              <time
                dateTime={e.date}
                style={{
                  color: '#6b6456',
                  fontSize: 12,
                  fontWeight: 500,
                  gridColumnStart: 1,
                  paddingTop: 8,
                  textAlign: 'right',
                  whiteSpace: 'nowrap',
                }}
              >
                {formatDate(e.date)}
              </time>

              <span
                aria-hidden
                style={{
                  alignItems: 'center',
                  background: '#FBF7ED',
                  border: '1px solid #D9D3C4',
                  borderRadius: '50%',
                  display: 'flex',
                  gridColumnStart: 2,
                  height: 12,
                  justifyContent: 'center',
                  justifySelf: 'center',
                  marginTop: 7,
                  position: 'relative',
                  width: 12,
                  zIndex: 1,
                }}
              >
                <span
                  style={{
                    background: '#1a1a1a',
                    borderRadius: '50%',
                    height: 4,
                    width: 4,
                  }}
                />
              </span>

              <div style={{ gridColumnStart: 3, minWidth: 0 }}>
                <h2
                  style={{
                    fontSize: 24,
                    fontWeight: 600,
                    letterSpacing: '-0.01em',
                    lineHeight: 1.2,
                    margin: '0 0 16px',
                  }}
                >
                  <a href={`#${e.version}`} style={{ color: 'inherit' }}>
                    {e.title}
                  </a>
                </h2>

                <ChangelogBody>{e.body}</ChangelogBody>

                {e.authors.length > 0 && (
                  <p
                    style={{
                      color: '#6b6456',
                      fontSize: 12,
                      marginTop: 28,
                    }}
                  >
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
