'use client';

import { useCallback, useEffect, useState } from 'react';

// List item shape the server component passes in — deliberately body-free
// (bodies dwarf the rest of the feed; the article view fetches its own).
export interface ChangelogListEntry {
  date: string;
  lede: string;
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

// ---- release channels --------------------------------------------------------
type Channel = { key: string; label: string };

// Classify a version string into a release channel from its pre-release suffix.
// Note: BAML currently has no stable channel. Plain `X.Y.Z` (no suffix) is the
// `canary` cut — the periodic milestone pulled from a nightly. The microservice
// also returns an authoritative `channel` field on each entry, but classifying
// from the version keeps things working even if `channel` is absent.
function channelOf(version: string): Channel {
  if (/-nightly\./i.test(version)) return { key: 'nightly', label: 'Nightly' };
  if (/-alpha\./i.test(version)) return { key: 'alpha', label: 'Alpha' };
  if (/-(beta|rc)[.\d]/i.test(version)) {
    return { key: 'prerelease', label: 'Pre-release' };
  }
  return { key: 'canary', label: 'Canary' };
}

// Canary-first ordering for the filter bar.
const CHANNEL_ORDER = ['canary', 'nightly', 'alpha', 'prerelease'];

// ---- shiki markdown pipeline (client-side, lazy) -----------------------------
// Built once on first use and reused. Dynamically imported so the (heavy) shiki
// + unified bundle only loads when a reader actually opens an article — the list
// view stays light, and the page itself remains static HTML (no 500 risk).
let processorPromise: Promise<(body: string) => Promise<string>> | null = null;

function getRenderer(): Promise<(body: string) => Promise<string>> {
  if (!processorPromise) {
    processorPromise = (async () => {
      const [
        { unified },
        { default: remarkParse },
        { default: remarkGfm },
        { default: remarkRehype },
        { default: rehypeStringify },
        { default: rehypeShikiFromHighlighter },
        { createHighlighter },
        { createJavaScriptRegexEngine },
        { default: bamlGrammar },
      ] = await Promise.all([
        import('unified'),
        import('remark-parse'),
        import('remark-gfm'),
        import('remark-rehype'),
        import('rehype-stringify'),
        // The /core entry lets us supply our OWN highlighter, so we can choose
        // the engine below.
        import('@shikijs/rehype/core'),
        import('shiki'),
        // The JavaScript regex engine avoids shiki's default oniguruma WASM,
        // which fails to load in the browser bundle — that silent failure was
        // why code blocks rendered unhighlighted. Pure-JS works client-side.
        import('shiki/engine/javascript'),
        import('@/lib/mdx/bamlTextmate.json'),
      ]);

      const highlighter = await createHighlighter({
        engine: createJavaScriptRegexEngine({ forgiving: true }),
        langs: [
          'bash',
          'json',
          'python',
          'rust',
          'tsx',
          'typescript',
          'yaml',
          // The raw TextMate grammar works directly as a shiki LanguageInput.
          {
            ...(bamlGrammar as Record<string, unknown>),
            aliases: [],
            name: 'baml',
          },
        ] as never,
        themes: ['github-light'],
      });

      const md = unified()
        .use(remarkParse)
        .use(remarkGfm)
        .use(remarkRehype)
        .use(rehypeShikiFromHighlighter, highlighter, {
          fallbackLanguage: 'text',
          theme: 'github-light',
        })
        .use(rehypeStringify);

      return async (body: string) => String(await md.process(body));
    })();
  }
  return processorPromise;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function Article({
  entry,
  onBack,
}: {
  entry: ChangelogListEntry;
  onBack: () => void;
}) {
  const [authors, setAuthors] = useState<string[]>([]);
  const [html, setHtml] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  // The list payload has no bodies, so fetch this release's full entry, then
  // render its markdown. Shiki failures degrade to escaped plain text.
  useEffect(() => {
    let alive = true;
    fetch(`/api/changelog-feed/entries/${encodeURIComponent(entry.version)}`)
      .then((r) =>
        r.ok ? r.json() : Promise.reject(new Error('bad status')),
      )
      .then(async (full: { authors: string[]; body: string }) => {
        if (!alive) return;
        setAuthors(full.authors ?? []);
        const h = await getRenderer()
          .then((render) => render(full.body))
          .catch(() => `<pre>${escapeHtml(full.body)}</pre>`);
        if (alive) setHtml(h);
      })
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [entry.version]);

  return (
    <article>
      <button type="button" onClick={onBack} className="chlog-back">
        ← All releases
      </button>

      <p className="chlog-meta">
        <span>{formatDate(entry.date)}</span>
        <span className="chlog-ver">{entry.version}</span>
        {(() => {
          const c = channelOf(entry.version);
          return (
            <span className={`chlog-tag chlog-tag--${c.key}`}>{c.label}</span>
          );
        })()}
      </p>
      <h1 className="chlog-article-title">{entry.title}</h1>

      {failed ? (
        <p style={{ color: '#6b6456' }}>
          Could not load this release. Please try again.
        </p>
      ) : html === null ? (
        <p style={{ color: '#6b6456' }}>Loading…</p>
      ) : (
        // eslint-disable-next-line react/no-danger
        <div className="chlog-md" dangerouslySetInnerHTML={{ __html: html }} />
      )}

      {authors.length > 0 && (
        <p className="chlog-authors">By {authors.join(', ')}</p>
      )}
    </article>
  );
}

export function ChangelogList({
  entries,
}: {
  entries: ChangelogListEntry[];
}) {
  const [selected, setSelected] = useState<string | null>(null);
  // Default the view to the Canary channel (the recommended channel).
  const [filter, setFilter] = useState<string | null>('canary');

  // Selected article is driven by the `?v=` query param so URLs are shareable
  // and the browser back button works — all client-side, page stays static.
  useEffect(() => {
    const sync = () =>
      setSelected(new URLSearchParams(window.location.search).get('v'));
    sync();
    window.addEventListener('popstate', sync);
    return () => window.removeEventListener('popstate', sync);
  }, []);

  const open = useCallback((version: string) => {
    window.history.pushState(
      {},
      '',
      `/changelog?v=${encodeURIComponent(version)}`,
    );
    setSelected(version);
    window.scrollTo({ top: 0 });
  }, []);

  const back = useCallback(() => {
    window.history.pushState({}, '', '/changelog');
    setSelected(null);
  }, []);

  const active = selected
    ? entries.find((e) => e.version === selected)
    : undefined;

  if (active) {
    return <Article entry={active} onBack={back} />;
  }

  // Channels actually present in the feed, stable-first — drives the filter bar.
  const present = CHANNEL_ORDER.filter((key) =>
    entries.some((e) => channelOf(e.version).key === key),
  );
  const shown = filter
    ? entries.filter((e) => channelOf(e.version).key === filter)
    : entries;

  return (
    <>
      <header className="chlog-header">
        <h1 className="chlog-h1">Changelog</h1>
        <p className="chlog-sub">The latest releases of BAML.</p>
      </header>

      {present.length > 1 && (
        <div
          className="chlog-filters"
          role="group"
          aria-label="Filter by channel"
        >
          <button
            type="button"
            className={`chlog-filter${filter === null ? ' is-active' : ''}`}
            onClick={() => setFilter(null)}
          >
            All
          </button>
          {present.map((key) => {
            const label = channelOf(
              entries.find((e) => channelOf(e.version).key === key)!.version,
            ).label;
            return (
              <button
                type="button"
                key={key}
                className={`chlog-filter chlog-filter--${key}${
                  filter === key ? ' is-active' : ''
                }`}
                onClick={() => setFilter(key)}
              >
                {label}
              </button>
            );
          })}
        </div>
      )}

      {shown.length === 0 ? (
        <p style={{ color: '#6b6456' }}>No entries yet.</p>
      ) : (
        <ol className="chlog-timeline">
          {/* continuous vertical rail running through every dot */}
          <div aria-hidden className="chlog-rail" />
          {shown.map((e) => {
            const c = channelOf(e.version);
            return (
              <li key={e.version} className="chlog-tl-item">
                <time dateTime={e.date} className="chlog-tl-date">
                  {formatDate(e.date)}
                </time>
                <span aria-hidden className="chlog-tl-dot">
                  <span />
                </span>
                <div className="chlog-tl-content">
                  <div className="chlog-tl-head">
                    <button
                      type="button"
                      className="chlog-tl-title"
                      onClick={() => open(e.version)}
                    >
                      {e.title}
                    </button>
                    <span className={`chlog-tag chlog-tag--${c.key}`}>
                      {c.label}
                    </span>
                  </div>
                  <p className="chlog-tl-lede">{e.lede}</p>
                  <button
                    type="button"
                    className="chlog-tl-read"
                    onClick={() => open(e.version)}
                  >
                    Read release →
                  </button>
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </>
  );
}
