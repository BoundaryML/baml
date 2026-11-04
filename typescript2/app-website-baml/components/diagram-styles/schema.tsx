'use client';

import { useLayoutEffect, useRef, useState } from 'react';
import { Beam, Card, Frame, useBox, useCycle, wire } from './kit';

// The type in the source and the value it produced, side by side. One field at a time lights up in both,
// with a wire between them, so the reader sees that every field of the output was declared. The sample
// compiles with baml check; the output is example data.
const SOURCE = [
  'type Label = "billing" | "bug" | "other";',
  '',
  'class Verdict {',
  '  label: Label,',
  '  confidence: float,',
  '  reason: string,',
  '}',
  '',
  'function classify(text: string) -> Verdict {',
  '  client: "openai/gpt-4o"',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: this is BAML source, shown as text
  '  prompt: `Classify this support ticket: ${text}`',
  '}',
];
const FIELDS = [3, 4, 5];

const TOKENS =
  /("[^"]*"|`[^`]*`|\b(?:type|class|function|client|prompt)\b|\b(?:string|float|int|bool)\b|\b[A-Z]\w*\b)/;

function Line({ text }: { text: string }) {
  return text.split(TOKENS).map((part, i) => {
    const key = `${i}-${part}`;
    if (!(i % 2)) return part;
    const tone = /^["`]/.test(part)
      ? 'text-(--ds-ok)'
      : /^[a-z]/.test(part)
        ? /^(string|float|int|bool)$/.test(part)
          ? 'text-(--ds-ink)'
          : 'text-(--ds-accent-ink)'
        : 'text-(--ds-ink)';
    return (
      <span className={tone} key={key}>
        {part}
      </span>
    );
  });
}

export default function Schema() {
  const active = useCycle(FIELDS.length, 2200, 0);
  const [ref, box] = useBox<HTMLDivElement>();
  const lines = useRef<(HTMLDivElement | null)[]>([]);
  const rows = useRef<(HTMLDivElement | null)[]>([]);
  const [d, setD] = useState('');

  useLayoutEffect(() => {
    const root = ref.current?.getBoundingClientRect();
    const from = lines.current[FIELDS[active]]?.getBoundingClientRect();
    const to = rows.current[active]?.getBoundingClientRect();
    if (!root || !from || !to || box.w < 768) {
      setD('');
      return;
    }
    setD(
      wire(
        from.right - root.left,
        from.top + from.height / 2 - root.top,
        to.left - root.left,
        to.top + to.height / 2 - root.top,
      ),
    );
  }, [active, box, ref]);

  return (
    <Frame label="A BAML source file declaring a Verdict class with a label, a confidence, and a reason, next to the Verdict that classify returned. Each field lights up in both, joined by a wire.">
      <div
        className="relative grid @min-[768px]:grid-cols-[minmax(0,1.25fr)_minmax(0,0.75fr)] items-center gap-6 @min-[768px]:gap-20 p-4 @min-[640px]:p-8 @min-[768px]:p-12"
        ref={ref}
      >
        <Card className="overflow-hidden">
          <div className="flex items-center gap-2 h-10 px-4 border-b border-(--ds-ink)/[0.08] text-[12.5px] text-(--ds-ink)/60">
            triage.baml
          </div>
          <div className="py-3 overflow-x-auto font-mono text-[11.5px] @min-[640px]:text-[13px] leading-[22px] text-(--ds-ink)/55">
            {SOURCE.map((text, i) => (
              <div
                className={`flex pr-4 transition-colors duration-300 ${FIELDS[active] === i ? 'bg-(--ds-accent)/10' : ''}`}
                // biome-ignore lint/suspicious/noArrayIndexKey: source lines keep their places
                key={i}
                ref={(el) => {
                  lines.current[i] = el;
                }}
              >
                <span className="w-9 shrink-0 pr-3 text-right text-(--ds-ink)/20 select-none">
                  {i + 1}
                </span>
                <span className="whitespace-pre">
                  <Line text={text} />
                </span>
              </div>
            ))}
          </div>
        </Card>

        <Card>
          <div className="flex items-center justify-between h-10 px-4 border-b border-(--ds-ink)/[0.08] text-[12.5px]">
            <span className="text-(--ds-ink)/60">Output</span>
            <span className="font-mono text-(--ds-ink)/35">Verdict</span>
          </div>
          <div className="p-2">
            {[
              [
                'label',
                <span
                  className="px-2 py-0.5 rounded-md bg-(--ds-accent)/12 text-(--ds-accent-ink) font-mono text-[12px]"
                  key="v"
                >
                  billing
                </span>,
              ],
              [
                'confidence',
                <span className="flex items-center gap-2" key="v">
                  <span className="relative w-16 h-1.5 rounded-full bg-(--ds-ink)/10 overflow-hidden">
                    <span className="absolute inset-y-0 left-0 w-[94%] rounded-full bg-(--ds-accent)" />
                  </span>
                  <span className="font-mono text-[12px] text-(--ds-ink)/80">
                    0.94
                  </span>
                </span>,
              ],
              [
                'reason',
                <span
                  className="text-[12.5px] text-(--ds-ink)/70 text-right"
                  key="v"
                >
                  Third refund request
                </span>,
              ],
            ].map(([name, value], i) => (
              <div
                className={`flex items-center justify-between gap-4 h-12 px-3 rounded-lg transition-colors duration-300 ${active === i ? 'bg-(--ds-ink)/[0.05]' : ''}`}
                key={name as string}
                ref={(el) => {
                  rows.current[i] = el;
                }}
              >
                <span
                  className={`font-mono text-[12.5px] transition-colors duration-300 ${active === i ? 'text-(--ds-ink)' : 'text-(--ds-ink)/45'}`}
                >
                  {name}
                </span>
                {value}
              </div>
            ))}
          </div>
        </Card>

        {d && (
          <svg
            aria-hidden="true"
            className="absolute inset-0 pointer-events-none"
            height={box.h}
            width={box.w}
          >
            <path
              d={d}
              fill="none"
              stroke="rgb(from var(--ds-accent) r g b / 0.35)"
              strokeDasharray="3 4"
            />
            <g key={active}>
              <Beam d={d} dur={1.6} />
            </g>
            <circle
              cx={d.split(' ').at(-2)}
              cy={d.split(' ').at(-1)}
              fill="var(--ds-accent)"
              r="3"
            />
          </svg>
        )}
      </div>
    </Frame>
  );
}
