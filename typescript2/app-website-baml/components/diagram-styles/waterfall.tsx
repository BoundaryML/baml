'use client';

import { Chip, Frame, useCycle } from './kit';

// A trace as BAML records it, drawn like an observability product's waterfall. Spans fill in as the
// trace plays, and the model call's typed output opens beside it. The trace is example data.
const TOTAL = 2410;
const SPANS = [
  { depth: 0, end: 2410, kind: 'fn', name: 'triage', start: 0 },
  { depth: 1, end: 380, kind: 'fn', name: 'plan', start: 0 },
  {
    depth: 1,
    end: 1212,
    kind: 'llm',
    model: 'gpt-4o',
    name: 'classify',
    start: 400,
  },
  { depth: 2, end: 1216, kind: 'parse', name: 'parse Verdict', start: 1212 },
  {
    depth: 1,
    end: 2380,
    kind: 'llm',
    model: 'claude',
    name: 'reply',
    start: 1240,
  },
  { depth: 2, end: 2386, kind: 'parse', name: 'parse string', start: 2380 },
];
const SELECTED = 2;
const STEPS = SPANS.length + 6;

const fill = (kind: string) =>
  kind === 'llm'
    ? 'var(--ds-accent)'
    : kind === 'parse'
      ? 'var(--ds-ok)'
      : 'rgb(from var(--ds-ink) r g b / 0.28)';

export default function Waterfall() {
  const step = useCycle(STEPS, 700);
  const open = step >= SPANS.length;

  return (
    <Frame label="A trace of the triage function: plan, then classify calling gpt-4o, then reply calling claude, each model call followed by a parse. The classify span is open, showing its input and its typed output, a Verdict with label billing and confidence 0.94.">
      <div className="p-3 @min-[640px]:p-6 @min-[768px]:p-8">
        <div className="rounded-xl border border-(--ds-line) bg-(--ds-card) overflow-hidden">
          <div className="flex flex-wrap items-center gap-2 px-4 h-auto py-3 border-b border-(--ds-ink)/[0.08]">
            <span className="mr-auto text-[13px] text-(--ds-ink)/85">
              Trace <span className="font-mono text-(--ds-ink)/50">7f3a2c</span>
            </span>
            <Chip>6 spans</Chip>
            <Chip>2.41s</Chip>
            <Chip className="hidden @min-[640px]:inline-flex">$0.0042</Chip>
          </div>

          <div className="grid @min-[768px]:grid-cols-[minmax(0,1fr)_320px]">
            <div className="py-2">
              <div className="grid grid-cols-[140px_minmax(0,1fr)] @min-[640px]:grid-cols-[180px_minmax(0,1fr)] px-4 pb-2 text-[11px] text-(--ds-ink)/35">
                <span>Span</span>
                <span className="flex justify-between">
                  <span>0ms</span>
                  <span>1.2s</span>
                  <span>2.4s</span>
                </span>
              </div>
              {SPANS.map((s, i) => (
                <div
                  className={`grid grid-cols-[140px_minmax(0,1fr)] @min-[640px]:grid-cols-[180px_minmax(0,1fr)] items-center h-9 px-4 transition-colors duration-300 ${open && i === SELECTED ? 'bg-(--ds-ink)/[0.04]' : ''}`}
                  key={s.name}
                >
                  <span
                    className="flex items-center gap-2 min-w-0 text-[12.5px] text-(--ds-ink)/80"
                    style={{ paddingLeft: s.depth * 14 }}
                  >
                    <span
                      className="size-1.5 shrink-0 rounded-full"
                      style={{ background: fill(s.kind) }}
                    />
                    <span className="truncate font-mono">{s.name}</span>
                    {s.model && (
                      <span className="hidden @min-[640px]:inline text-[11px] text-(--ds-ink)/35">
                        {s.model}
                      </span>
                    )}
                  </span>
                  <span className="relative h-full border-l border-(--ds-ink)/[0.06]">
                    {step >= i && (
                      <span
                        className="ds-grow absolute top-1/2 -translate-y-1/2 h-2.5 rounded-[3px]"
                        key={`${s.name}-${step >= i}`}
                        style={{
                          background: fill(s.kind),
                          left: `${(s.start / TOTAL) * 100}%`,
                          minWidth: 3,
                          opacity: s.kind === 'fn' ? 1 : 0.85,
                          width: `${((s.end - s.start) / TOTAL) * 100}%`,
                        }}
                      />
                    )}
                  </span>
                </div>
              ))}
            </div>

            <div className="border-t @min-[768px]:border-t-0 @min-[768px]:border-l border-(--ds-ink)/[0.08] p-4 min-h-[232px]">
              {open ? (
                <div className="ds-in" key="open">
                  <p className="flex items-center justify-between text-[13px] text-(--ds-ink)/85">
                    <span className="font-mono">classify</span>
                    <span className="text-(--ds-ink)/40">812ms, gpt-4o</span>
                  </p>
                  <p className="mt-4 text-[11px] text-(--ds-ink)/35">Input</p>
                  <p className="mt-1 font-mono text-[12px] text-(--ds-ok)">
                    "Third time asking. Refund me."
                  </p>
                  <p className="mt-4 text-[11px] text-(--ds-ink)/35">
                    Output, Verdict
                  </p>
                  <pre className="mt-1 font-mono text-[12px] leading-5 text-(--ds-ink)/80">
                    {'{\n  label: '}
                    <span className="text-(--ds-accent-ink)">"billing"</span>
                    {',\n  confidence: '}
                    <span className="text-(--ds-accent-ink)">0.94</span>
                    {'\n}'}
                  </pre>
                  <p className="mt-4 text-[11px] text-(--ds-ink)/35">
                    412 in, 18 out tokens
                  </p>
                </div>
              ) : (
                <p className="text-[12px] text-(--ds-ink)/30">
                  Select a span to see its input and output.
                </p>
              )}
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
