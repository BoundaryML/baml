'use client';

import { useEffect, useState } from 'react';
import {
  BAML_CSV_TESTS,
  BAML_HTTP_TESTS,
  BAML_IMAGE,
  BAML_MATCH,
  BAML_PACKED,
  BAML_RUNNER,
  BAML_SENTIMENT,
  BAML_SPAWN,
  BAML_SPAWN_ADV,
  BAML_UNKNOWN,
  BAML_UNREACHABLE,
  BAML_WF_FANOUT,
  BAML_WF_TALLY,
  BENCH_BAML,
  DESCRIBE_EVENTS,
  GREP_EVENTS,
  NS_BAD,
  NS_GOOD,
  PACK_BENCH,
  PACK_EVENTS,
  RUN_E_EVENTS,
  RUN_FN_EVENTS,
  SPAWN_BENCH,
  TS_CATCH,
  TS_INSTANCEOF,
  TS_LIES,
} from '@/app/baml-intro/_components/snippets';
import BamlEditor from '@/app/learn2/_components/baml-editor-lazy';
import LivePlayground from '@/app/learn2/_components/LivePlaygroundLazy';
import type { TermEvent } from '@/app/learn3/_components/TermPlay';

/* Fills each section's "interactive explorer" box with code from the
 * /explore article, verbatim: every snippet is imported from its snippets.ts
 * (compiler-verified there), and the terminal transcripts and benchmark
 * tables are its captured CLI output and measurements. Tabs with no /explore
 * source were removed rather than paraphrased. */

/* ---- rendering helpers ---- */

function termText(events: TermEvent[]): string {
  return events
    .map((e) => (e.cmd !== undefined ? `$ ${e.cmd}` : (e.text ?? '')))
    .join('\n');
}

const PACK_BENCH_TEXT = [
  '# same hello world, both compiled to one self-contained binary',
  '#                    size      gzip      startup',
  ...PACK_BENCH.map(
    (r) =>
      `${r.tool.padEnd(20)} ${r.size.padEnd(9)} ${r.gzip.padEnd(9)} ${r.startup}${r.accent ? '   <--' : ''}`,
  ),
].join('\n');

const SPAWN_BENCH_TEXT = [
  '# 38.4 GB of text scanned: 16 shards x 50 rounds x 48 MB',
  '#                     time     cpu',
  ...SPAWN_BENCH.map(
    (r) =>
      `${r.run.padEnd(21)} ${r.time.padEnd(8)} ${r.cpu}${r.accent ? '   <--' : ''}`,
  ),
].join('\n');

type Block = {
  /** Small label above the block (e.g. "TypeScript" / "BAML"). */
  label?: string;
  lang: 'baml' | 'typescript' | 'term';
  code: string;
  /** Mount this block as the live playground (editable + runnable),
   *  preselecting `fn` in the run panel. */
  fn?: string;
};

type Panel = { tab: string; blocks: Block[] };

const EXAMPLES: Record<string, Panel[]> = {
  'ai-functions': [
    {
      blocks: [{ code: BAML_SENTIMENT, fn: 'classify', lang: 'baml' }],
      tab: 'AI Functions',
    },
    {
      blocks: [{ code: BAML_IMAGE, fn: 'illustrate', lang: 'baml' }],
      tab: 'AI Classes',
    },
  ],
  evals: [
    {
      blocks: [
        {
          code: BAML_CSV_TESTS,
          fn: 'classify',
          label: 'from a CSV',
          lang: 'baml',
        },
        {
          code: BAML_HTTP_TESTS,
          label: 'from object storage, at collection time',
          lang: 'baml',
        },
      ],
      tab: 'Tests from Data',
    },
    {
      blocks: [{ code: BAML_RUNNER, fn: 'check_inventory', lang: 'baml' }],
      tab: 'Handle Flakiness',
    },
  ],
  language: [
    {
      blocks: [
        { code: TS_LIES, label: 'TypeScript', lang: 'typescript' },
        { code: BAML_UNKNOWN, fn: 'load', label: 'BAML', lang: 'baml' },
      ],
      tab: 'No Any',
    },
    {
      blocks: [
        { code: TS_INSTANCEOF, label: 'TypeScript', lang: 'typescript' },
        { code: BAML_MATCH, fn: 'route', label: 'BAML', lang: 'baml' },
      ],
      tab: 'switch < match',
    },
    {
      blocks: [
        { code: TS_CATCH, label: 'TypeScript', lang: 'typescript' },
        { code: BAML_UNREACHABLE, fn: 'show', label: 'BAML', lang: 'baml' },
      ],
      tab: 'Typed Errors',
    },
    {
      blocks: [
        { code: NS_BAD, label: "doesn't compile", lang: 'baml' },
        { code: NS_GOOD, label: 'compiles', lang: 'baml' },
      ],
      tab: 'Local Reasoning',
    },
  ],
  observability: [
    {
      blocks: [{ code: BAML_WF_FANOUT, fn: 'analyze', lang: 'baml' }],
      tab: 'Always-on Observability',
    },
    {
      blocks: [
        {
          code: BAML_WF_TALLY,
          fn: 'summarize',
          label: '//# annotations label the trace and the graph',
          lang: 'baml',
        },
      ],
      tab: 'Data Enrichment',
    },
    {
      blocks: [
        {
          code: BAML_SPAWN,
          fn: 'main',
          label: 'every call and thread is a span an agent can read back',
          lang: 'baml',
        },
      ],
      tab: 'Agents Using Traces',
    },
    {
      blocks: [
        {
          code: BENCH_BAML,
          fn: 'par',
          label: 'the benchmark source',
          lang: 'baml',
        },
        { code: SPAWN_BENCH_TEXT, label: 'measured', lang: 'term' },
      ],
      tab: 'Runs at Scale',
    },
  ],
  tooling: [
    {
      blocks: [{ code: termText(RUN_FN_EVENTS), lang: 'term' }],
      tab: 'Compile Faster',
    },
    {
      blocks: [
        { code: termText(GREP_EVENTS), label: 'grep', lang: 'term' },
        {
          code: termText(DESCRIBE_EVENTS),
          label: 'baml describe',
          lang: 'term',
        },
      ],
      tab: 'Search Better',
    },
    {
      blocks: [{ code: termText(RUN_E_EVENTS), lang: 'term' }],
      tab: 'Run Directly',
    },
    {
      blocks: [
        { code: BAML_PACKED, fn: 'main', label: 'main.baml', lang: 'baml' },
        { code: termText(PACK_EVENTS), label: 'baml pack', lang: 'term' },
        { code: PACK_BENCH_TEXT, label: 'measured', lang: 'term' },
      ],
      tab: 'Ship Anywhere',
    },
  ],
  workflows: [
    {
      blocks: [{ code: BAML_SPAWN, fn: 'main', lang: 'baml' }],
      tab: 'No Function Coloring',
    },
    {
      blocks: [{ code: BAML_SPAWN_ADV, fn: 'main', lang: 'baml' }],
      tab: 'Limit Concurrency',
    },
  ],
};

/* ---- shiki highlighting (same approach as /explore's vs-client) ---- */

type CodeToken = { content: string; color?: string };
type CodeTokens = CodeToken[][];

function plainTokens(code: string): CodeTokens {
  return code.split('\n').map((line) => [{ content: line }]);
}

function useTokenized(blocks: Block[]): CodeTokens[] {
  const [out, setOut] = useState<CodeTokens[]>(() =>
    blocks.map((b) => plainTokens(b.code)),
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: tokenize once per mount; the block list is static per section
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { createHighlighter } = await import('shiki');
        const { bamlTextmate, bamlJinjaTextmate } = await import(
          '@/lib/mdx/shiki-grammars'
        );
        const highlighter = await createHighlighter({
          langs: ['typescript', bamlJinjaTextmate, bamlTextmate],
          themes: ['github-light'],
        });
        const results = blocks.map((b) => {
          if (b.lang === 'term') return plainTokens(b.code);
          const r = highlighter.codeToTokens(b.code, {
            lang: b.lang as never,
            theme: 'github-light',
          });
          return r.tokens.map((line) =>
            line.map((t) => ({ color: t.color, content: t.content })),
          );
        });
        if (!cancelled) setOut(results);
      } catch {
        /* keep plain text */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return out;
}

const HINT_KEY = 'wib-x-expand-hint-seen';

function LiveBlock({
  code,
  fn,
  hint,
  label,
}: {
  code: string;
  fn: string;
  /** First explorer on the page nudges toward the expand affordance. */
  hint?: boolean;
  label: string;
}) {
  const [expanded, setExpanded] = useState(false);
  // Escape closes the overlay and the page behind it stops scrolling while
  // the dialog is up.
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setExpanded(false);
    };
    document.addEventListener('keydown', onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prev;
    };
  }, [expanded]);
  // Hydration-safe: the hint appears after mount, and only until the visitor
  // has expanded a playground once (persisted).
  const [showHint, setShowHint] = useState(false);
  useEffect(() => {
    if (!hint) return;
    try {
      if (!localStorage.getItem(HINT_KEY)) setShowHint(true);
    } catch {
      setShowHint(true);
    }
  }, [hint]);
  const expand = () => {
    setExpanded(true);
    setShowHint(false);
    try {
      localStorage.setItem(HINT_KEY, '1');
    } catch {
      /* private mode */
    }
  };
  return (
    <div className="wib-x-slot">
      <div className="wib-x-livecode">
        {showHint ? (
          <span aria-hidden className="wib-x-hintchip">
            expand to see the full playground &rarr;
          </span>
        ) : null}
        <button
          aria-label="Expand into the playground"
          className={`wib-x-expand${showHint ? ' glow' : ''}`}
          onClick={expand}
          title="Expand: edit + run in the playground"
          type="button"
        >
          {'\u2922'}
        </button>
        {/* The playground's own single-pane editor: real diagnostics, hover,
            run codelenses; each cell gets an isolated project. */}
        <div className="wib-x-editorframe">
          <BamlEditor codeLens={false} initialCode={code} maxHeight={620} />
        </div>
      </div>
      {expanded ? (
        <>
          <button
            aria-label="Close expanded playground"
            className="wib-x-ov"
            onClick={() => setExpanded(false)}
            type="button"
          />
          <div aria-modal="true" className="wib-x-live ex" role="dialog">
            <button
              aria-label="Collapse playground"
              className="wib-x-expand"
              onClick={() => setExpanded(false)}
              title="Collapse"
              type="button"
            >
              {'\u2921'}
            </button>
            {/* The playground only exists while expanded: one worker at a
                time instead of one per section at page load. */}
            <LivePlayground
              fill
              initialCode={code}
              initialFunction={fn}
              initialSidebarOpen={false}
              initialTab="run"
              isolated
              loadingLabel={`Loading ${label}…`}
            />
          </div>
        </>
      ) : null}
    </div>
  );
}

function TermBlock({ code }: { code: string }) {
  return (
    <pre className="wib-x-term">
      {code.split('\n').map((line, i) => (
        <div
          className={
            line.startsWith('$ ')
              ? 'cmd'
              : line.startsWith('#')
                ? 'dim'
                : undefined
          }
          // biome-ignore lint/suspicious/noArrayIndexKey: static transcript
          key={i}
        >
          {line || '​'}
        </div>
      ))}
    </pre>
  );
}

function CodeBlock({ tokens }: { tokens: CodeTokens }) {
  return (
    <div className="wib-x-codewrap">
      <div aria-hidden className="wib-x-gutter">
        {tokens.map((_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static snippet
          <div key={i}>{i + 1}</div>
        ))}
      </div>
      <pre className="wib-x-code">
        {tokens.map((line, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static snippet
          <div key={i}>
            {line.length === 0 ? (
              <span>{'​'}</span>
            ) : (
              line.map((t, j) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: static snippet
                <span key={j} style={t.color ? { color: t.color } : undefined}>
                  {t.content}
                </span>
              ))
            )}
          </div>
        ))}
      </pre>
    </div>
  );
}

export function CodeExplorer({
  readMore,
  sectionId,
}: {
  readMore?: string;
  sectionId: string;
}) {
  const panels = EXAMPLES[sectionId];
  const [active, setActive] = useState(0);
  const allBlocks = (panels ?? []).flatMap((p) => p.blocks);
  const tokens = useTokenized(allBlocks);
  if (!panels) return null;

  const panel = panels[Math.min(active, panels.length - 1)];
  const panelId = `wib-x-panel-${sectionId}`;
  const tabId = (i: number) => `wib-x-tab-${sectionId}-${i}`;
  const offset = panels
    .slice(0, panels.indexOf(panel))
    .reduce((n, p) => n + p.blocks.length, 0);

  return (
    <div className="wib-x">
      <div aria-label="Examples" className="wib-x-tabs" role="tablist">
        {panels.map((p, i) => (
          <button
            aria-controls={panelId}
            aria-selected={i === active}
            className={`wib-x-tab${i === active ? ' on' : ''}`}
            id={tabId(i)}
            key={p.tab}
            onClick={() => setActive(i)}
            role="tab"
            type="button"
          >
            {p.tab}
          </button>
        ))}
      </div>
      <div
        aria-labelledby={tabId(Math.min(active, panels.length - 1))}
        className="wib-x-body"
        id={panelId}
        role="tabpanel"
      >
        {panel.blocks.map((b, i) => (
          <div className="wib-x-block" key={`${panel.tab}-${b.label ?? i}`}>
            {b.label ? <div className="wib-x-label">{b.label}</div> : null}
            {b.lang === 'term' ? (
              <TermBlock code={b.code} />
            ) : b.fn ? (
              /* isolated inside: several sections mount a playground at once,
                 and the page-shared worker would clobber their projects. */
              <LiveBlock
                code={b.code}
                fn={b.fn}
                hint={sectionId === 'observability'}
                key={`${sectionId}-${panel.tab}`}
                label={panel.tab}
              />
            ) : (
              <CodeBlock tokens={tokens[offset + i] ?? plainTokens(b.code)} />
            )}
          </div>
        ))}
      </div>
      {readMore ? (
        <p className="wib-x-more">
          Read more &rarr; <a href="/techdocs">{readMore}</a>
        </p>
      ) : null}
      <style>{`
        .wib-x { border: 1px solid #D9D3C4; border-radius: 12px; background: #FDFBF6; margin-top: var(--sp-5, 20px); overflow: hidden; }
        .wib-x-tabs { display: flex; flex-wrap: wrap; gap: 8px; padding: 12px 16px 0; }
        .wib-x-tab { background: #FFFFFF; border: 1px solid #D9D3C4; border-radius: 999px; color: #5C5852; cursor: pointer; font-family: inherit; font-size: 14px; padding: 6px 14px 7px; }
        .wib-x-tab:hover { border-color: #7C3AED; color: #7C3AED; }
        .wib-x-tab.on { background: #F6F2FF; border-color: #7C3AED; color: #7C3AED; font-weight: 600; }
        .wib-x-body { display: grid; gap: 12px; padding: 14px 16px 16px; }
        .wib-x-more { display: flex; align-items: baseline; justify-content: flex-end; gap: 6px; margin: 0; padding: 0 16px 14px; font-size: 15px; font-weight: 600; }
        .wib-x-more a { color: #6D28D9; text-decoration: none; }
        .wib-x-more a:hover { text-decoration: underline; }
        .wib-x-label { color: #5C5852; font-family: inherit; font-size: 13.5px; font-weight: 600; margin-bottom: 6px; }
        /* static panes mirror the Monaco editor frame: white ground, numbered
           gutter, same 13px/20px metrics */
        .wib-x-editorframe { background: #FFFDF7; border: 1px solid #E7E1D2; border-radius: 8px; overflow: hidden; }
        .wib-x-codewrap { background: #FFFDF7; border: 1px solid #E7E1D2; border-radius: 8px; display: flex; max-height: 480px; overflow: auto; }
        .wib-x-gutter { color: #B8B2A6; flex-shrink: 0; font-family: var(--font-geist-mono), ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; line-height: 20px; padding: 13px 0; text-align: right; user-select: none; width: 46px; }
        .wib-x-code { background: transparent; border: none; color: #1A1612; flex: 1; font-family: var(--font-geist-mono), ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; line-height: 20px; margin: 0; min-width: 0; overflow: visible; padding: 13px 15px 13px 20px; }
        .wib-x-term { background: #1A1612; border: 1px solid #1A1612; border-radius: 8px; color: #E8E3D8; font-family: var(--font-geist-mono), ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12.5px; line-height: 1.55; margin: 0; max-height: 480px; overflow: auto; padding: 13px 15px; }
        .wib-x-term { background: #1A1612; border-color: #1A1612; color: #E8E3D8; }
        .wib-x-term .cmd { color: #C4B5FD; font-weight: 600; }
        .wib-x-term .dim { color: #8A8580; }
        .wib-x-slot { position: relative; }
        .wib-x-livecode { position: relative; }
        .wib-x-live { border: 1px solid #E7E1D2; border-radius: 8px; overflow: hidden; position: relative; }
        .wib-x-live .l2-live, .wib-x-live > .baml-playground-root, .wib-x-live > div:last-child { height: 100%; }
        /* expanded: a centered overlay holding the (freshly mounted) playground */
        .wib-x-live.ex { background: #FDFBF6; box-shadow: 0 24px 80px -12px rgba(26, 22, 18, 0.45); height: min(85vh, 900px); left: 50%; position: fixed; top: 50%; transform: translate(-50%, -50%); width: min(1280px, 94vw); z-index: 60; }
        .wib-x-ov { background: rgba(26, 22, 18, 0.45); border: none; cursor: zoom-out; inset: 0; position: fixed; z-index: 59; }
        .wib-x-expand { align-items: center; background: #FFFFFF; border: 1px solid #D9D3C4; border-radius: 6px; color: #5C5852; cursor: pointer; display: inline-flex; font-size: 15px; height: 26px; justify-content: center; line-height: 1; position: absolute; right: 8px; top: 8px; width: 26px; z-index: 5; }
        .wib-x-expand:hover { border-color: #7C3AED; color: #7C3AED; }
        /* no expanded playground on touch/small screens: the overlay + Monaco
           on mobile is chaos, so the affordance simply is not there */
        @media (max-width: 700px), (pointer: coarse) { .wib-x-expand, .wib-x-hintchip { display: none; } }
        .wib-x-expand.glow { animation: wib-x-glow 1.6s ease-in-out infinite; border-color: #7C3AED; color: #7C3AED; }
        @keyframes wib-x-glow {
          0%, 100% { box-shadow: 0 0 0 0 rgba(124, 58, 237, 0.45); }
          50% { box-shadow: 0 0 0 7px rgba(124, 58, 237, 0); }
        }
        .wib-x-hintchip { background: #F6F2FF; border: 1px solid #DACCF7; border-radius: 999px; color: #7C3AED; font-family: inherit; font-size: 12.5px; padding: 4px 11px 5px; position: absolute; right: 42px; top: 9px; white-space: nowrap; z-index: 5; }
      `}</style>
    </div>
  );
}
