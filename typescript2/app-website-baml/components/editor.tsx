/** biome-ignore-all lint/suspicious/noTemplateCurlyInString: ${...} is BAML's interpolation in the sample code */
/** biome-ignore-all lint/suspicious/noSuspiciousSemicolonInJsx: the semicolons are the sample code's */
/** biome-ignore-all lint/correctness/useJsxKeyInIterable: static lines, keyed by index where they render */
/** biome-ignore-all lint/suspicious/noArrayIndexKey: the lines never reorder */
'use client';

import { type ReactNode, useState } from 'react';
import { track } from './track';
import { K, S, T } from './ui';

// Rendered by index below, so the array items need no keys.
// biome-ignore format: the spaces inside these lines are the rendered code's indentation (.code-line is pre-wrap)
const LINES: ReactNode[] = [
  <><K>type</K> <T>Label</T> = <S>&quot;positive&quot;</S> | <S>&quot;negative&quot;</S> | <S>&quot;neutral&quot;</S>;</>,
  "",
  <><K>class</K> <T>Verdict</T> {"{"}</>,
  <>  label: <T>Label</T>,</>,
  <>  confidence: <T>float</T>,</>,
  "}",
  "",
  <><K>function</K> <span className="text-white font-semibold">classify</span>(text: <T>string</T>) -&gt; <T>Verdict</T> {"{"}</>,
  <>  client: <S>&quot;openai/gpt-4o&quot;</S></>,
  <>  prompt: <S>`</S></>,
  <S>    Classify the sentiment of the text. Sarcasm counts</S>,
  <S>    as the sentiment actually expressed.</S>,
  <><S>    Text: </S><K>{"${text}"}</K></>,
  <>    <K>{"${ctx.output_format()}"}</K></>,
  <S>  `</S>,
  "}",
];

// Tab labels; the example is illustrative.
const FILES = ['classify.baml', 'main.py', 'app.ts'] as const;

export default function Editor() {
  const [ran, setRan] = useState(true);
  const run = () => {
    track('editor_run_clicked');
    setRan(false);
    setTimeout(() => setRan(true), 450);
  };

  return (
    <div className="flex flex-col w-full h-full rounded-xl bg-raised border border-white/10 shadow-[0_32px_64px_-16px_rgba(5,4,20,0.7)] overflow-hidden">
      <div className="flex items-center justify-between h-11 pl-1.5 pr-2 border-b border-white/8 font-mono text-xs">
        <div className="flex gap-0.5">
          {FILES.map((f, i) => (
            <span
              className={`px-2.5 py-1.5 rounded-md ${i === 0 ? 'bg-white/7 text-ink' : 'text-muted'} ${i === 2 ? 'max-sm:hidden' : ''}`}
              key={f}
            >
              {f}
            </span>
          ))}
        </div>
        <button
          aria-label="Run classify"
          className="tap shrink-0 whitespace-nowrap px-2.5 py-1.5 rounded-md border border-clover/40 text-clover font-sans font-medium hover:bg-clover/10 transition-colors"
          onClick={run}
          type="button"
        >
          ▶ Run
        </button>
      </div>

      {/* The number lives inside its line rather than in a parallel column: that is what lets a long
          prompt line wrap under itself on a phone instead of dragging a scrollbar across the panel. */}
      <div className="code-pane flex-1 px-4 sm:px-5 py-[22px] font-mono text-[12.5px] sm:text-[13px] leading-[23px]">
        <pre className="text-[#c9c3d3] [--hang:30px]">
          {LINES.map((l, i) => (
            <div className="code-line" key={i}>
              <span
                aria-hidden
                className="inline-block w-[18px] mr-3 text-right text-faint select-none"
              >
                {i + 1}
              </span>
              {l || ' '}
            </div>
          ))}
        </pre>
      </div>

      <div className="flex flex-col gap-1.5 px-5 pt-4 pb-[18px] border-t border-white/8 bg-clover/5 font-mono text-[12.5px] leading-5">
        <span className="text-muted">
          $ baml run classify -- --text &quot;Oh great, another Monday.&quot;
        </span>
        <span aria-live="polite" className="text-[#c9c3d3] min-h-5">
          {ran ? (
            <>
              <T>Verdict</T> {'{'} label: <S>&quot;negative&quot;</S>,
              confidence: <K>0.94</K> {'}'}
            </>
          ) : (
            <span className="caret text-muted">▍</span>
          )}
        </span>
      </div>
    </div>
  );
}
