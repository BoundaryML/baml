'use client';

import { createContext, type ReactNode, useContext, useState } from 'react';
import {
  LangIcon,
  LangPicker,
  usePinnedLang,
} from '@/components/home/lang-pick';
import { BOOK } from '@/components/home/links';
import { ArchitecturePage } from './architecture';
import { LANGS, SAMPLES } from './code';

// The /adopt page: the homepage's three ways in, small, then each one in full, then how BAML runs.
// The language picked anywhere on the page is the language everywhere on it.

// ---------- one BAML function, called from every language ----------

// The language picked anywhere on the page is the language everywhere on the page.
const Lang = createContext<{ at: number; setAt: (i: number) => void }>({
  at: 0,
  setAt: () => {},
});

// Highlighted HTML for every sample, keyed by its source, made on the server by the page.
const Html = createContext<Record<string, string>>({});

function Pane({
  children,
  file,
  lang,
}: {
  children: string;
  file: string;
  // The language the file is in, for its logo.
  lang?: string;
}) {
  const html = useContext(Html)[children];
  return (
    <div className="flex min-w-0 flex-col overflow-hidden rounded-xl border border-rule bg-raised">
      <div className="flex h-10 items-center border-b border-rule px-3.5">
        <span className="flex items-center gap-2 font-mono text-[12px] text-ink-2">
          {lang && <LangIcon className="size-3.5" name={lang} />}
          {file}
        </span>
      </div>
      {html ? (
        <div
          className="flex-1 overflow-x-auto px-3.5 py-3 font-mono text-[12px] leading-[1.65] [&_pre]:!bg-transparent [&_code]:font-mono"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: Shiki's output for our own fixed samples
          dangerouslySetInnerHTML={{ __html: html }}
        />
      ) : (
        <pre className="flex-1 overflow-x-auto px-3.5 py-3 font-mono text-[12.5px] leading-[1.65] text-body">
          {children}
        </pre>
      )}
    </div>
  );
}

/** The incremental route as one picture: the BAML function on the left, the code that calls it on
 *  the right in whichever language you pick, and the one command between them. */
function Incremental() {
  const { at, setAt } = useContext(Lang);
  const lang = LANGS[at];
  const sample = SAMPLES[useContext(Sample).at];
  return (
    <div>
      <LangPicker at={at} setAt={setAt} />
      <div className="mt-3 grid gap-3 lg:grid-cols-2">
        <Pane file={`baml_src/${sample.file}`}>{sample.baml}</Pane>
        <Pane file={lang.file} lang={lang.name}>
          {sample.calls[lang.name] ?? lang.call}
        </Pane>
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 rounded-xl border border-rule bg-raised px-3.5 py-2.5 font-mono text-[12.5px]">
        <span className="text-ink">
          <span className="text-muted">$ </span>baml generate add {lang.target}{' '}
          &amp;&amp; baml generate
        </span>
        <span className="text-muted">then {lang.install}</span>
      </div>
    </div>
  );
}

const Sample = createContext<{ at: number; setAt: (i: number) => void }>({
  at: 0,
  setAt: () => {},
});

/** The list of samples, beside the incremental route's text: picking one swaps the code on the right. */
function SamplePicker() {
  const { at, setAt } = useContext(Sample);
  return (
    <div className="mt-6">
      <p className="font-mono text-[12px] text-muted">Try it with</p>
      <div className="mt-2 space-y-1.5">
        {SAMPLES.map((sm, i) => {
          const on = i === at;
          return (
            <button
              aria-pressed={on}
              className={`focus-ring block w-full cursor-pointer rounded-lg border px-3 py-2.5 text-left transition-colors ${on ? 'border-clover bg-clover/10' : 'border-white/10 hover:border-white/25'}`}
              key={sm.name}
              onClick={() => setAt(i)}
              type="button"
            >
              <span
                className={`block text-[15px] ${on ? 'text-head' : 'text-ink-2'}`}
              >
                {sm.name}
              </span>
              <span className="mt-0.5 block text-[13px] leading-snug text-muted">
                {sm.note}
              </span>
            </button>
          );
        })}
      </div>
    </div>
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

const PRODUCTION: Step = {
  body: (
    <>
      BAML is open source (Apache 2.0), and on your machine it records every
      call with no account. For hyperobservability in production,{' '}
      <EarlyAccess>get early access</EarlyAccess> to Boundary Cloud.
    </>
  ),
};

const ROUTES: {
  after: string;
  id: string;
  // Something under the route's text, in the space beside its steps.
  aside?: ReactNode;
  // A route can draw its own first step, ahead of the listed ones.
  custom?: ReactNode;
  steps: Step[];
  title: string;
  recommended?: boolean;
  // Why we'd hold off, or what to know first, under its own label.
  warning?: { label: string; text: ReactNode };
  when: string;
}[] = [
  {
    after:
      'Those functions get hyperobservability. Boundary can trace the rest of your code too, so you can move your whole tracing stack off OTel.',
    aside: <SamplePicker />,
    custom: <Incremental />,
    id: 'incremental',
    recommended: true,
    steps: [PRODUCTION],
    title: 'Adopt BAML incrementally',
    when: 'BAML generates a typed SDK for most languages, so your app calls it like its own code.',
  },
  {
    after:
      'Every service lands in Boundary Cloud: one place to see and query what your backend does.',
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
    after:
      'Everything you write is hyperobservable from the first run. Nothing else to deploy next to it.',
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
      PRODUCTION,
    ],
    title: 'Write new services in BAML',
    warning: {
      label: 'Why not rewrite yet',
      text: (
        <>
          We don&apos;t recommend rewriting existing services yet. BAML&apos;s
          standard library and package ecosystem are still young, and a rewrite
          trades code that works for risk. Move the functions that benefit most
          first, and save full BAML for new services.
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
      className="grid scroll-mt-24 gap-6 border-t border-rule py-10 md:grid-cols-[minmax(0,20rem)_minmax(0,1fr)] md:gap-12"
      id={r.id}
    >
      <div>
        {r.recommended && (
          <span className="mb-3 inline-flex items-center gap-1.5 rounded-full border border-clover/60 px-2.5 py-1 text-[12px] font-medium text-clover">
            <span className="size-1.5 rounded-full bg-clover" />
            Recommended
          </span>
        )}
        <h3 className="v4-h3 text-head">{r.title}</h3>
        <p className="mt-3 v4-body text-ink-2">{r.when}</p>
        {r.aside}
        {r.warning && (
          <p className="mt-4 rounded-lg border border-[#f5c451]/40 bg-[#f5c451]/[0.06] px-3.5 py-3 text-[14px] leading-snug text-ink-2">
            <span className="font-mono text-[11px] uppercase tracking-wider text-[#f5c451]">
              {r.warning.label}
            </span>
            <br />
            {r.warning.text}
          </p>
        )}
      </div>
      <div>
        <ol className="space-y-4">
          {r.custom && (
            <li className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-3">
              <StepNumber n={1} />
              <div className="min-w-0">{r.custom}</div>
            </li>
          )}
          {r.steps.map((s, i) => (
            <li
              className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-3"
              key={s.code ?? String(i)}
            >
              <StepNumber n={i + (r.custom ? 2 : 1)} />
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
        <p className="mt-5 rounded-lg border border-clover/40 bg-clover/[0.06] px-4 py-3 text-[15px] leading-snug text-head">
          <span className="font-mono text-[12px] uppercase tracking-wider text-clover">
            What you get
          </span>
          <br />
          {r.after}
        </p>
      </div>
    </div>
  );
}

/** The /adopt page: the three ways in, each in full, then how BAML runs for anyone who wants
 *  the detail. `html` is every code sample, highlighted on the server (./highlight.ts). */
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
            <nav aria-label="On this page" className="mt-8 md:mt-10">
              <ol className="grid gap-px overflow-hidden rounded-xl border border-rule bg-rule sm:grid-cols-2 lg:grid-cols-4">
                {[
                  ...ROUTES.map((r) => ({
                    href: `#${r.id}`,
                    label: r.title,
                    recommended: r.recommended,
                  })),
                  {
                    href: '#how-baml-runs',
                    label: 'How BAML runs',
                    recommended: false,
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
                      <span className="flex-1">
                        {item.label}
                        {item.recommended && (
                          <span className="ml-2 inline-block size-1.5 -translate-y-0.5 rounded-full bg-clover" />
                        )}
                      </span>
                      <span
                        aria-hidden
                        className="text-muted transition-[translate] group-hover:translate-y-0.5"
                      >
                        ↓
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
          <section
            className="v3-wrap mx-auto scroll-mt-24 border-t border-rule px-(--frame-pad) py-20 md:py-28"
            id="how-baml-runs"
          >
            <ArchitecturePage embedded />
          </section>
        </Sample.Provider>
      </Lang.Provider>
    </Html.Provider>
  );
}
