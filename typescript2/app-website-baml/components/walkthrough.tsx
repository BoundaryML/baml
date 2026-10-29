/** biome-ignore-all lint/correctness/useJsxKeyInIterable: static lines, keyed by index where they render */
/** biome-ignore-all lint/suspicious/noArrayIndexKey: the lines never reorder */
/** biome-ignore-all lint/suspicious/noSuspiciousSemicolonInJsx: the semicolons are the sample code's */
/** biome-ignore-all lint/complexity/noUselessFragments: each fragment is one line of sample code */
'use client';

import { type ReactNode, useState } from 'react';
import { section } from './analytics';
import Reveal from './reveal';
import { track } from './track';
import { C, K, LANES, LEFT, PAD_Y, S, T, WRAP } from './ui';
import useTabKeys from './use-tab-keys';

/* One concrete example walked end to end, deliberately the same `classify` function the hero editor
   shows, so the page demonstrates one thing rather than three. The code is illustrative; every
   capability in it (baml generate, typed clients for Python and TypeScript, the single-file
   function) is stated on boundaryml.com. */

// Every list below is rendered by index, so the items need no keys of their own.
// biome-ignore format: the spaces inside these lines are the rendered code's indentation (.code-line is pre-wrap)
const DEFINE: ReactNode[] = [
  <><K>class</K> <T>Verdict</T> {"{"}</>,
  <>  label: <T>Label</T>,</>,
  <>  confidence: <T>float</T>,</>,
  "}",
  "",
  <><K>function</K> <span className="text-white font-semibold">classify</span>(text: <T>string</T>) -&gt; <T>Verdict</T> {"{"}</>,
  <>  client: <S>&quot;openai/gpt-4o&quot;</S></>,
  <>  prompt: <S>`Classify the sentiment …`</S></>,
  "}",
];

// biome-ignore format: the spaces inside these lines are the rendered code's indentation (.code-line is pre-wrap)
const GENERATE: ReactNode[] = [
  <><C>$</C> baml generate</>,
  <C>  baml_client/ · typescript, python</C>,
  "",
  <><C>$</C> baml test classify</>,
  <C>  2 tests · 2 passed</C>,
];

// biome-ignore format: the spaces inside these lines are the rendered code's indentation (.code-line is pre-wrap)
// biome-ignore assist/source/useSortedKeys: TypeScript first, the same order as LANGS
const CALL = {
  typescript: {
    file: "app.ts",
    lines: [
      <><K>import</K> {"{ b }"} <K>from</K> <S>&quot;./baml_client&quot;</S>;</>,
      "",
      <><K>const</K> v = <K>await</K> b.<T>classify</T>(</>,
      <>  <S>&quot;Oh great, another Monday.&quot;</S>,</>,
      <>);</>,
      <C>{"// v: Verdict — label is a union, not a string"}</C>,
      "",
      <><K>if</K> (v.label === <S>&quot;negative&quot;</S>) <T>escalate</T>(v);</>,
    ],
  },
  python: {
    file: "main.py",
    lines: [
      <><K>from</K> baml_client <K>import</K> b</>,
      "",
      <>v = <K>await</K> b.<T>classify</T>(</>,
      <>  <S>&quot;Oh great, another Monday.&quot;</S>,</>,
      <>)</>,
      <C># v: Verdict — label is a union, not a str</C>,
      "",
      <><K>if</K> v.label == <S>&quot;negative&quot;</S>: <T>escalate</T>(v)</>,
    ],
  },
} as const;

type Lang = keyof typeof CALL;
const LANGS = ['typescript', 'python'] as const;

// min-w-0 twice over: a grid item sizes to its content by default, and the code panes scroll, so
// without it a long line pushes the whole column past the viewport on a phone.
const PANEL =
  'on-paint h-full min-w-0 flex flex-col rounded-xl bg-raised border border-white/10 overflow-hidden';
const CHROME =
  'flex items-center justify-between gap-3 h-11 px-3 border-b border-white/8 font-mono text-xs';
const CODE =
  'code-pane flex-1 px-[18px] py-4 font-mono text-[12.5px] leading-[22px] text-[#c9c3d3]';

// Each line wraps under itself, so the panel never scrolls sideways on a phone.
const lines = (ls: readonly ReactNode[]) =>
  ls.map((l, i) => (
    <div className="code-line" key={i}>
      {l || ' '}
    </div>
  ));

function Step({
  title,
  caption,
  children,
}: {
  title: string;
  caption: ReactNode;
  children: ReactNode;
}) {
  return (
    <li className="min-w-0 flex flex-col gap-4">
      <h3 className="font-display font-medium text-[21px] leading-[27px] tracking-[-0.03em]">
        {title}
      </h3>
      {children}
      <p className="text-[15px] leading-[23px] text-body">{caption}</p>
    </li>
  );
}

export default function Walkthrough() {
  const [lang, setLang] = useState<Lang>('typescript');
  const keys = useTabKeys(LANGS, lang, setLang);
  const call = CALL[lang];

  return (
    <section
      {...section('walkthrough')}
      aria-labelledby="walkthrough-title"
      className={`${WRAP} ${PAD_Y}`}
    >
      <div className={`${LANES} mb-10 md:mb-14 lg:mb-16`}>
        <div className={`${LEFT} flex flex-col gap-4`}>
          <h2 className="h2" id="walkthrough-title">
            One function, start to finish.
          </h2>
        </div>
        <p className="max-w-[520px] lg:pt-2.5 text-[17px] leading-[27px] text-body">
          The same{' '}
          <span className="font-mono text-[0.94em] text-ink-2">classify</span>{' '}
          function from the top of this page: written once in BAML, compiled
          into a typed client, called from the app you already ship.
        </p>
      </div>

      <ol className="grid lg:grid-cols-3 gap-x-10 gap-y-11 lg:gap-y-14">
        <Step
          caption="The type, the model and the prompt live in one file, so a change is one diff."
          title="Write the function"
        >
          <Reveal className={PANEL}>
            <div className={CHROME}>
              <span className="px-2 py-1 rounded-md bg-white/7 text-ink">
                classify.baml
              </span>
            </div>
            <pre className={CODE}>{lines(DEFINE)}</pre>
          </Reveal>
        </Step>

        <Step
          caption="One command writes a typed client for every language in the project, and runs the tests that live beside the function."
          title="Generate the client"
        >
          <Reveal className={PANEL} delay={80}>
            <div className={CHROME}>
              <span className="px-2 py-1 rounded-md bg-white/7 text-ink">
                zsh
              </span>
              <span className="text-muted">~/support-triage</span>
            </div>
            <pre className={CODE}>{lines(GENERATE)}</pre>
          </Reveal>
        </Step>

        <Step
          caption="The result arrives typed in your language. No parsing step, no casts, and nothing else in the app has to change."
          title="Call it from your app"
        >
          <Reveal className={PANEL} delay={160}>
            <div className={CHROME}>
              <div
                aria-label="Generated client"
                className="flex gap-0.5"
                onKeyDown={keys.onKeyDown}
                role="tablist"
              >
                {LANGS.map((l) => (
                  <button
                    aria-controls="call-panel"
                    aria-selected={lang === l}
                    className={`tap px-2 py-1 rounded-md transition-colors ${lang === l ? 'bg-white/7 text-ink' : 'text-muted hover:text-body'}`}
                    id={`call-tab-${l}`}
                    key={l}
                    onClick={() => {
                      track('walkthrough_language_selected', { language: l });
                      setLang(l);
                    }}
                    role="tab"
                    tabIndex={keys.tabIndex(l)}
                    type="button"
                  >
                    {CALL[l].file}
                  </button>
                ))}
              </div>
            </div>
            <pre
              aria-labelledby={`call-tab-${lang}`}
              className={CODE}
              id="call-panel"
              role="tabpanel"
            >
              {lines(call.lines)}
            </pre>
          </Reveal>
        </Step>
      </ol>
    </section>
  );
}
