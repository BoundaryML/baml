'use client';

import { createContext, type ReactNode, useContext, useState } from 'react';
import { LAMB_PATHS } from '@/components/home/lamb';
import {
  LangIcon,
  LangPicker,
  usePinnedLang,
} from '@/components/home/lang-pick';
import { BOOK } from '@/components/home/links';
import { LANGS, SAMPLES } from './code';
import { Start } from './start';

// The /adopt page: the homepage's three ways in, small, then each one in full. How BAML works is
// its own page, /how-baml-works, linked from the contents.
// The language picked anywhere on the page is the language everywhere on it.

// ---------- one BAML function, called from every language ----------

// The language picked anywhere on the page is the language everywhere on the page.
const Lang = createContext<{ at: number; setAt: (i: number) => void }>({
  at: 0,
  setAt: () => {},
});

// Highlighted HTML for every sample, keyed by its source, made on the server by the page.
const Html = createContext<Record<string, string>>({});

/** The BAML lamb, at the size of the language logos, for the BAML file. */
function Lamb() {
  return (
    <svg
      aria-hidden="true"
      className="h-3.5 w-auto shrink-0 text-clover"
      fill="currentColor"
      viewBox="0 0 356 422"
    >
      {LAMB_PATHS.map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}

/** One file in the figure: its name, then its code. The figure draws the frame around both. */
function Pane({
  children,
  file,
  lang,
}: {
  children: string;
  file: string;
  // The language the file is in, for its logo; a BAML file gets the lamb.
  lang?: string;
}) {
  const html = useContext(Html)[children];
  return (
    <div className="flex min-w-0 flex-col">
      <span className="flex items-center gap-2 px-4 pt-3 font-mono text-[11.5px] text-muted">
        {lang ? <LangIcon className="size-3.5" name={lang} /> : <Lamb />}
        {file}
      </span>
      {html ? (
        <div
          className="flex-1 overflow-x-auto px-4 pt-2 pb-4 font-mono text-[12.5px] leading-[1.65] [&_pre]:!bg-transparent [&_code]:font-mono"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: Shiki's output for our own fixed samples
          dangerouslySetInnerHTML={{ __html: html }}
        />
      ) : (
        <pre className="flex-1 overflow-x-auto px-4 pt-2 pb-4 font-mono text-[12.5px] leading-[1.65] text-body">
          {children}
        </pre>
      )}
    </div>
  );
}

const Sample = createContext<{ at: number; setAt: (i: number) => void }>({
  at: 0,
  setAt: () => {},
});

/** The incremental route as one framed picture: what to try and your language along the top, the
 *  BAML function and the code that calls it side by side, and the one command along the bottom. */
function Incremental() {
  const { at, setAt } = useContext(Lang);
  const picked = useContext(Sample);
  const lang = LANGS[at];
  const sample = SAMPLES[picked.at];
  return (
    <figure>
      <div className="overflow-hidden rounded-xl border border-rule bg-raised">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-rule px-2 py-1.5">
          <div className="flex flex-wrap items-center gap-0.5">
            {SAMPLES.map((sm, i) => {
              const on = i === picked.at;
              return (
                <button
                  aria-pressed={on}
                  className={`focus-ring h-7 cursor-pointer rounded-md px-2.5 text-[13px] transition-colors ${on ? 'bg-white/10 text-head' : 'text-muted hover:text-ink'}`}
                  key={sm.name}
                  onClick={() => picked.setAt(i)}
                  type="button"
                >
                  {sm.name}
                </button>
              );
            })}
          </div>
          <LangPicker at={at} compact setAt={setAt} />
        </div>
        <div className="grid divide-rule max-lg:divide-y lg:grid-cols-2 lg:divide-x">
          <Pane file={`baml_src/${sample.file}`}>{sample.baml}</Pane>
          <Pane file={lang.file} lang={lang.name}>
            {sample.calls[lang.name] ?? lang.call}
          </Pane>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-rule px-4 py-2.5 font-mono text-[12.5px]">
          <span className="text-ink">
            <span className="text-muted">$ </span>baml generate add{' '}
            {lang.target} &amp;&amp; baml generate
          </span>
          <span className="text-muted">then {lang.install}</span>
        </div>
      </div>
      <figcaption className="mt-3 text-[14px] text-muted">
        {sample.note}
      </figcaption>
    </figure>
  );
}

// ---------- the guide: three routes, the recommended one first ----------

type Step = { body: ReactNode; code?: string };

/** Boundary Cloud's early access, which every route goes through: first for OTel, last for the
 *  others, where it's what gets you hyperobservability in production. */
function EarlyAccess({ children }: { children: ReactNode }) {
  return (
    <a
      className="text-clover underline underline-offset-2 hover:text-head"
      href={BOOK}
      rel="noopener"
      target="_blank"
    >
      {children}
    </a>
  );
}

/** What BAML costs and where its recordings go, as three plain rows: the same for every route that
 *  runs BAML. */
function Facts() {
  const rows: [string, ReactNode][] = [
    ['Open source', 'Apache 2.0'],
    ['Hyperobservability locally', 'SQLite'],
    [
      'Hyperobservability in production',
      <EarlyAccess key="cloud">Boundary Cloud, in early access</EarlyAccess>,
    ],
  ];
  return (
    <dl className="mt-6 divide-y divide-rule border-y border-rule">
      {rows.map(([k, v]) => (
        <div className="py-2.5" key={k}>
          <dt className="text-[12.5px] text-muted">{k}</dt>
          <dd className="mt-0.5 text-[14.5px] text-ink">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

const ROUTES: {
  id: string;
  // A route can show one picture in place of steps.
  figure?: ReactNode;
  // Shows the open source, local and production rows under the route's text.
  facts?: boolean;
  steps: Step[];
  title: string;
  recommended?: boolean;
  // Why we'd hold off, or what to know first, under its own label.
  warning?: { label: string; text: ReactNode };
  when: string;
}[] = [
  {
    facts: true,
    figure: <Incremental />,
    id: 'incremental',
    recommended: true,
    steps: [],
    title: 'Adopt BAML incrementally',
    when: 'BAML generates a typed SDK for most languages, so your app calls it like its own code.',
  },
  {
    id: 'otel',
    steps: [
      {
        body: (
          <>
            Boundary Cloud is in early access.{' '}
            <EarlyAccess>Get early access</EarlyAccess> to start.
          </>
        ),
      },
      {
        body: 'Point the OpenTelemetry exporter your services already use at Boundary Cloud.',
        code: 'OTEL_EXPORTER_OTLP_ENDPOINT=<your Boundary Cloud endpoint>',
      },
      {
        body: 'Redeploy. No code changes, in any language.',
      },
    ],
    title: 'Send your OTel traces to Boundary Cloud',
    warning: {
      label: 'Good to know',
      text: 'Since it’s OTel, we can’t do much for performance or storage.',
    },
    when: 'Works with whatever your backend is written in.',
  },
  {
    facts: true,
    id: 'rewrite',
    steps: [
      { body: 'Start a project.', code: 'baml new my-service' },
      {
        body: 'Run any function or script.',
        code: 'baml run -f handle_ticket',
      },
      {
        body: 'Ship it as one self-contained binary, no host language.',
        code: 'baml pack -f handle_ticket',
      },
    ],
    title: 'Write new services in BAML',
    warning: {
      label: 'Don’t rewrite your codebase in BAML yet',
      text: (
        <>
          Its standard library and package ecosystem are still young. Keep the
          code that works, and move over only the functions that gain the most.
        </>
      ),
    },
    when: 'For new services, where there is nothing to migrate.',
  },
];

function StepNumber({ n }: { n: number }) {
  return (
    <span className="grid size-6 place-items-center rounded-full border border-white/20 font-mono text-[12px] text-ink-2">
      {n}
    </span>
  );
}

function Route({ r }: { r: (typeof ROUTES)[number] }) {
  return (
    <div
      className="grid scroll-mt-24 gap-8 border-t border-rule py-12 md:grid-cols-[minmax(0,19rem)_minmax(0,1fr)] md:gap-14"
      id={r.id}
    >
      <div>
        {r.recommended && (
          <span className="mb-3 inline-flex items-center gap-1.5 text-[13px] font-medium text-clover">
            <span className="size-1.5 rounded-full bg-clover" />
            Recommended
          </span>
        )}
        <h3 className="v4-h3 text-head">{r.title}</h3>
        <p className="mt-3 v4-body text-ink-2">{r.when}</p>
        {r.warning && (
          <p className="mt-5 border-l-2 border-[#f5c451]/60 pl-3.5 text-[14px] leading-relaxed text-ink-2">
            <span className="font-medium text-[#f5c451]">
              {r.warning.label}.
            </span>{' '}
            {r.warning.text}
          </p>
        )}
        {r.facts && <Facts />}
      </div>
      <div className="min-w-0">
        {r.figure}
        {r.steps.length > 0 && (
          <ol className="space-y-5">
            {r.steps.map((s, i) => (
              <li
                className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-3"
                key={s.code ?? String(i)}
              >
                <StepNumber n={i + 1} />
                <div className="min-w-0">
                  <p className="v4-body text-body">{s.body}</p>
                  {s.code && (
                    <pre className="mt-2 overflow-x-auto rounded-lg border border-rule bg-raised px-3.5 py-2.5 font-mono text-[13px] leading-[1.6] text-ink">
                      {s.code}
                    </pre>
                  )}
                </div>
              </li>
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}

/** The /adopt page: the three ways in, each in full. `html` is every code sample, highlighted on the server (./highlight.ts). */
export function AdoptGuide({ html }: { html: Record<string, string> }) {
  const [at, setAt] = usePinnedLang();
  const [sample, setSample] = useState(0);
  return (
    <Html.Provider value={html}>
      <Lang.Provider value={{ at, setAt }}>
        <Sample.Provider value={{ at: sample, setAt: setSample }}>
          <section className="v3-wrap mx-auto px-(--frame-pad) pt-16 pb-20 md:pt-24 md:pb-28">
            <h1 className="v4-h2 max-w-[22em] text-head">
              Adopting Boundary and BAML
            </h1>
            <Start />
            <nav aria-label="On this page" className="mt-10 md:mt-14">
              <ol className="grid gap-px overflow-hidden rounded-xl border border-rule bg-rule sm:grid-cols-2 lg:grid-cols-4">
                {[
                  ...ROUTES.map((r) => ({
                    href: `#${r.id}`,
                    label: r.title,
                  })),
                  {
                    href: '/how-baml-works',
                    label: 'How BAML works',
                  },
                ].map((item, i) => (
                  <li className="bg-ground" key={item.href}>
                    <a
                      className="focus-ring group flex h-full items-start gap-3 px-4 py-3.5 text-[15px] leading-snug text-ink-2 transition-colors hover:bg-raised hover:text-head"
                      href={item.href}
                    >
                      <span className="mt-0.5 font-mono text-[12px] text-muted">
                        {String(i + 1).padStart(2, '0')}
                      </span>
                      <span className="flex-1">{item.label}</span>
                      {/* A section below gets ↓; the separate How BAML works page gets →. */}
                      <span
                        aria-hidden
                        className={`text-muted transition-[translate] ${item.href.startsWith('/') ? 'group-hover:translate-x-0.5' : 'group-hover:translate-y-0.5'}`}
                      >
                        {item.href.startsWith('/') ? '→' : '↓'}
                      </span>
                    </a>
                  </li>
                ))}
              </ol>
            </nav>
            <div className="mt-8">
              {ROUTES.map((r) => (
                <Route key={r.id} r={r} />
              ))}
            </div>
          </section>
        </Sample.Provider>
      </Lang.Provider>
    </Html.Provider>
  );
}
