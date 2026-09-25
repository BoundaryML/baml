'use client';
import { useState } from 'react';
import type { Repro } from '@/lib/types';
import { cn } from '@/lib/utils';
import { ReproCard } from './repro-card';

function label(r: Repro, i: number): string {
  const what = i === 0 ? 'Repro' : `Comparison ${i}`;
  const result =
    r.result === 'fails'
      ? 'fails'
      : r.result === 'passes'
        ? 'passes'
        : r.result === 'invalid_repro'
          ? 'invalid example'
          : r.result === 'unsupported_verification'
            ? 'verification unavailable'
            : r.result === 'verification_error'
              ? 'execution error'
              : r.result === 'inconclusive'
                ? 'inconclusive'
                : null;
  return result ? `${what} · ${result}` : what;
}

/** One repro at a time: the primary first, comparisons behind tabs. */
export function ReproTabs({ repros }: { repros: Repro[] }) {
  const [active, setActive] = useState(0);
  const current = repros[Math.min(active, repros.length - 1)];
  if (!current) return null;
  return (
    <div>
      {repros.length > 1 && (
        <div className="mb-2 flex flex-wrap gap-1" role="tablist">
          {repros.map((r, i) => (
            <button
              aria-selected={i === active}
              className={cn(
                'rounded-md border px-2.5 py-1 text-xs',
                i === active
                  ? 'bg-foreground text-background'
                  : 'bg-card text-muted-foreground hover:text-foreground',
              )}
              // biome-ignore lint/suspicious/noArrayIndexKey: Repro positions are fixed for this issue and identify the active tab.
              key={i}
              onClick={() => setActive(i)}
              role="tab"
              type="button"
            >
              {label(r, i)}
              <span
                aria-hidden
                className={cn(
                  'ml-1.5 inline-block h-1.5 w-1.5 rounded-full align-middle',
                  r.result === 'fails'
                    ? 'bg-rose-500'
                    : r.result === 'passes'
                      ? 'bg-emerald-500'
                      : 'bg-slate-400',
                )}
              />
            </button>
          ))}
        </div>
      )}
      <ReproCard comparison={active > 0} repro={current} />
    </div>
  );
}
