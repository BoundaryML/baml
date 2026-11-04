'use client';

import { Beam, Card, Frame, GREEN, RED, useBox, wire } from './kit';

// Where an hour of calls to classify went, as a flow chart: most are answered by the primary model, a few
// time out and fall back to the second, and a handful fail. Band widths follow the share of calls, with a
// floor so the smallest stays visible. The numbers are example data.
const FLOWS = [
  {
    calls: 11981,
    color: 'var(--ds-accent)',
    model: 'gpt-4o',
    note: 'answered first time',
  },
  {
    calls: 449,
    color: GREEN,
    model: 'claude-sonnet',
    note: 'answered after a timeout',
  },
  { calls: 50, color: RED, model: 'Failed', note: 'both models errored' },
];
const TOTAL = FLOWS.reduce((a, f) => a + f.calls, 0);
const NODE = 10;
const GAP = 40;

/** A band from one vertical span to another, curved like a wire. */
function band(
  x1: number,
  a1: number,
  b1: number,
  x2: number,
  a2: number,
  b2: number,
) {
  const m = (x1 + x2) / 2;
  return `M${x1} ${a1}C${m} ${a1} ${m} ${a2} ${x2} ${a2}L${x2} ${b2}C${m} ${b2} ${m} ${b1} ${x1} ${b1}Z`;
}

const pct = (n: number) => `${((n / TOTAL) * 100).toFixed(1)}%`;

function Label({
  children,
  style,
}: {
  children: React.ReactNode;
  style: React.CSSProperties;
}) {
  return (
    <span
      className="absolute inline-flex items-center gap-2 h-7 px-2.5 rounded-md border border-(--ds-line) bg-(--ds-card) text-[12.5px] text-(--ds-ink)/85 whitespace-nowrap"
      style={style}
    >
      {children}
    </span>
  );
}

export default function Routing() {
  const [ref, { h, w }] = useBox<HTMLDivElement>();

  const thick = (calls: number) => Math.max(3, (calls / TOTAL) * h * 0.38);
  const sizes = FLOWS.map((f) => thick(f.calls));
  const cy = h / 2;
  const sx = 16;
  // On a phone the middle column moves left to make room for its labels, and the end labels drop to the
  // bottom corners so they stay clear of the model labels.
  const narrow = w < 500;
  const mx = Math.round(w * (narrow ? 0.36 : 0.46));
  const tx = w - 16 - NODE;

  // The source node holds every flow, stacked.
  const srcH = sizes.reduce((a, b) => a + b, 0);
  const src = sizes.map((t, i) => {
    const top = cy - srcH / 2 + sizes.slice(0, i).reduce((a, b) => a + b, 0);
    return [top, top + t];
  });

  // The model nodes sit apart in the middle column.
  let m = cy - (srcH + GAP * (FLOWS.length - 1)) / 2;
  const mid = sizes.map((t) => {
    const span = [m, m + t];
    m += t + GAP;
    return span;
  });

  // The output node takes the two answered flows.
  const outH = sizes[0] + sizes[1];
  const out = [
    [cy - outH / 2, cy - outH / 2 + sizes[0]],
    [cy - outH / 2 + sizes[0], cy + outH / 2],
  ];

  return (
    <Frame label="Calls to classify in the last hour as a flow. Of 12,480 calls, 96 percent were answered by gpt-4o, 3.6 percent fell back to claude-sonnet after a timeout, and 0.4 percent failed. The answered calls all return a Verdict.">
      <div className="p-3 @min-[640px]:p-8 @min-[768px]:p-12">
        <Card className="p-4 @min-[640px]:p-6">
          <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2">
            <p className="text-[14px] text-(--ds-ink)/85">
              <span className="font-mono">classify</span>
              <span className="text-(--ds-ink)/40">
                , where calls went in the last hour
              </span>
            </p>
            <p className="font-mono text-[12px] text-(--ds-ink)/40">
              fallback: gpt-4o, then claude-sonnet
            </p>
          </div>

          <div
            className="relative mt-4 h-[280px] @min-[640px]:h-[320px]"
            ref={ref}
          >
            {w > 0 && (
              <>
                <svg
                  aria-hidden="true"
                  className="absolute inset-0"
                  height={h}
                  width={w}
                >
                  {FLOWS.map((f, i) => (
                    <path
                      d={band(
                        sx + NODE,
                        src[i][0],
                        src[i][1],
                        mx,
                        mid[i][0],
                        mid[i][1],
                      )}
                      fill={f.color}
                      fillOpacity={0.2}
                      key={f.model}
                    />
                  ))}
                  {FLOWS.slice(0, 2).map((f, i) => (
                    <path
                      d={band(
                        mx + NODE,
                        mid[i][0],
                        mid[i][1],
                        tx,
                        out[i][0],
                        out[i][1],
                      )}
                      fill={f.color}
                      fillOpacity={0.2}
                      key={f.model}
                    />
                  ))}

                  {[0, 1.1, 2.2].map((delay) => (
                    <g key={delay}>
                      <Beam
                        d={wire(
                          sx + NODE,
                          (src[0][0] + src[0][1]) / 2,
                          mx,
                          (mid[0][0] + mid[0][1]) / 2,
                        )}
                        delay={delay}
                        dur={2.4}
                      />
                      <Beam
                        d={wire(
                          mx + NODE,
                          (mid[0][0] + mid[0][1]) / 2,
                          tx,
                          (out[0][0] + out[0][1]) / 2,
                        )}
                        delay={delay + 1.2}
                        dur={2.4}
                      />
                    </g>
                  ))}
                  <Beam
                    color={GREEN}
                    d={wire(
                      sx + NODE,
                      (src[1][0] + src[1][1]) / 2,
                      mx,
                      (mid[1][0] + mid[1][1]) / 2,
                    )}
                    delay={0.6}
                    dur={3.2}
                  />
                  <Beam
                    color={GREEN}
                    d={wire(
                      mx + NODE,
                      (mid[1][0] + mid[1][1]) / 2,
                      tx,
                      (out[1][0] + out[1][1]) / 2,
                    )}
                    delay={2.2}
                    dur={3.2}
                  />
                  <Beam
                    color={RED}
                    d={wire(
                      sx + NODE,
                      (src[2][0] + src[2][1]) / 2,
                      mx,
                      (mid[2][0] + mid[2][1]) / 2,
                    )}
                    delay={1.8}
                    dur={4}
                  />

                  <rect
                    fill="rgb(from var(--ds-ink) r g b / 0.85)"
                    height={srcH}
                    rx={2}
                    width={NODE}
                    x={sx}
                    y={src[0][0]}
                  />
                  {FLOWS.map((f, i) => (
                    <rect
                      fill={f.color}
                      height={mid[i][1] - mid[i][0]}
                      key={f.model}
                      rx={2}
                      width={NODE}
                      x={mx}
                      y={mid[i][0]}
                    />
                  ))}
                  <rect
                    fill="rgb(from var(--ds-ink) r g b / 0.85)"
                    height={outH}
                    rx={2}
                    width={NODE}
                    x={tx}
                    y={out[0][0]}
                  />
                </svg>

                <Label
                  style={{ left: sx, top: narrow ? h - 28 : src[0][0] - 38 }}
                >
                  classify{' '}
                  <span className="text-(--ds-ink)/45 tabular-nums">
                    {TOTAL.toLocaleString('en-US')}
                  </span>
                </Label>
                {FLOWS.map((f, i) => (
                  <Label
                    key={f.model}
                    style={{
                      left: mx + NODE + 8,
                      top: (mid[i][0] + mid[i][1]) / 2 - 14,
                    }}
                  >
                    <span
                      className="size-2 rounded-full"
                      style={{ background: f.color }}
                    />
                    <span className={i < 2 ? 'font-mono' : ''}>{f.model}</span>
                    <span className="text-(--ds-ink)/45 tabular-nums">
                      {pct(f.calls)}
                    </span>
                  </Label>
                ))}
                <Label
                  style={{ right: 16, top: narrow ? h - 28 : out[0][0] - 38 }}
                >
                  Verdict{' '}
                  <span className="text-(--ds-ink)/45 tabular-nums">
                    {pct(FLOWS[0].calls + FLOWS[1].calls)}
                  </span>
                </Label>
              </>
            )}
          </div>

          <dl className="grid grid-cols-3 gap-3 mt-4 pt-5 border-t border-(--ds-ink)/[0.06] text-[13px]">
            {FLOWS.map((f) => (
              <div key={f.model}>
                <dt
                  className={`text-(--ds-ink)/45 ${f.model === 'Failed' ? '' : 'font-mono'}`}
                >
                  {f.model}
                </dt>
                <dd className="mt-1 font-display font-medium text-[22px] tracking-[-0.03em] tabular-nums">
                  {f.calls.toLocaleString('en-US')}
                </dd>
                <dd className="text-[12px] text-(--ds-ink)/35">{f.note}</dd>
              </div>
            ))}
          </dl>
        </Card>
      </div>
    </Frame>
  );
}
