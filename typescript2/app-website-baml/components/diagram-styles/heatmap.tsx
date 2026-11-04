'use client';

import { useEffect, useState } from 'react';
import { Card, Frame, RED } from './kit';

// A day of calls to classify as a dot grid, one dot per batch. Failures build up through the morning,
// a deploy lands, and they stop. The grid fills in column by column, as if streaming. Example data.
const COLS = 48;
const ROWS = 7;
const DEPLOY = 31;

// Whether the batch at a column and row failed: rarely at first, more often toward the deploy, never after.
function failed(c: number, r: number) {
  if (c >= DEPLOY) return false;
  const h = Math.sin(c * 12.9898 + r * 78.233) * 43758.5453;
  return h - Math.floor(h) < 0.04 + (c / DEPLOY) ** 2 * 0.42;
}

export default function Heatmap() {
  const [shown, setShown] = useState(COLS);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    let n = 0;
    const id = setInterval(() => {
      n = (n + 1) % (COLS + 14);
      setShown(Math.min(n, COLS));
    }, 110);
    return () => clearInterval(id);
  }, []);

  const failures = Array.from(
    { length: Math.min(shown, DEPLOY) },
    (_, c) =>
      Array.from({ length: ROWS }, (_, r) => failed(c, r)).filter(Boolean)
        .length,
  ).reduce((a, b) => a + b, 0);

  return (
    <Frame label="A day of calls to classify as a grid of dots. Failed batches, in red, grow through the morning until v43 is deployed, after which every batch passes.">
      <div className="p-3 @min-[640px]:p-8 @min-[768px]:p-12">
        <Card className="p-4 @min-[640px]:p-6">
          <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2">
            <p className="text-[14px] text-(--ds-ink)/85">
              <span className="font-mono">classify</span>
              <span className="text-(--ds-ink)/40">
                , calls in the last 24 hours
              </span>
            </p>
            <p className="flex items-center gap-4 text-[12px] text-(--ds-ink)/45">
              <span className="flex items-center gap-1.5">
                <span className="size-2 rounded-full bg-(--ds-ink)/25" /> Passed
              </span>
              <span className="flex items-center gap-1.5">
                <span
                  className="size-2 rounded-full"
                  style={{ background: RED }}
                />{' '}
                Failed
              </span>
            </p>
          </div>

          <div className="relative mt-6 overflow-hidden">
            <div
              className="grid gap-[3px] @min-[640px]:gap-[5px]"
              style={{ gridTemplateColumns: `repeat(${COLS}, minmax(0, 1fr))` }}
            >
              {Array.from({ length: ROWS }, (_, r) =>
                Array.from({ length: COLS }, (_, c) => (
                  <span
                    className="aspect-square rounded-full transition-[opacity,background-color] duration-300"
                    // biome-ignore lint/suspicious/noArrayIndexKey: a dot is a fixed cell in the grid
                    key={`${r}-${c}`}
                    style={{
                      background: failed(c, r)
                        ? RED
                        : c >= DEPLOY
                          ? 'rgb(from var(--ds-accent) r g b / 0.55)'
                          : 'rgb(from var(--ds-ink) r g b / 0.22)',
                      opacity: c < shown ? 1 : 0.08,
                    }}
                  />
                )),
              )}
            </div>
            {/* The deploy, as a line through the grid. */}
            <span
              className={`absolute -top-1 -bottom-1 w-px bg-(--ds-accent) transition-opacity duration-300 ${shown > DEPLOY ? 'opacity-100' : 'opacity-0'}`}
              style={{ left: `${(DEPLOY / COLS) * 100}%` }}
            />
          </div>

          <div className="relative flex justify-between mt-3 font-mono text-[11px] text-(--ds-ink)/35">
            <span>00:00</span>
            <span className="hidden @min-[640px]:inline">12:00</span>
            <span>24:00</span>
            <span
              className={`absolute -top-0.5 -translate-x-1/2 px-2 py-0.5 rounded-md border border-(--ds-accent)/40 bg-(--ds-card) text-(--ds-accent-ink) transition-opacity duration-300 ${shown > DEPLOY ? 'opacity-100' : 'opacity-0'}`}
              style={{ left: `${(DEPLOY / COLS) * 100}%` }}
            >
              v43 deployed
            </span>
          </div>

          <div className="grid grid-cols-2 gap-3 mt-6 pt-5 border-t border-(--ds-ink)/[0.06] text-[13px]">
            <p className="text-(--ds-ink)/45">
              Failed before the fix
              <span className="block mt-1 font-display font-medium text-[24px] tracking-[-0.03em] text-(--ds-ink) tabular-nums">
                {failures}
              </span>
            </p>
            <p className="text-(--ds-ink)/45">
              Failed since v43
              <span className="block mt-1 font-display font-medium text-[24px] tracking-[-0.03em] text-(--ds-ink) tabular-nums">
                0
              </span>
            </p>
          </div>
        </Card>
      </div>
    </Frame>
  );
}
