'use client';

import Link from 'next/link';
import { type CSSProperties, useMemo, useState } from 'react';
import { useSolved } from '../_lib/progress';
import type { Difficulty } from '../_lib/types';

export interface BoardItem {
  slug: string;
  id: number;
  title: string;
  difficulty: Difficulty;
  category: string;
}

const DIFF_ABBR: Record<Difficulty, string> = {
  Easy: 'Easy',
  Medium: 'Med.',
  Hard: 'Hard',
};
const DIFF_CLASS: Record<Difficulty, string> = {
  Easy: 'bc-diff-easy',
  Medium: 'bc-diff-medium',
  Hard: 'bc-diff-hard',
};

// Deterministic, illustrative acceptance rate + frequency bars (LeetCode shows
// real telemetry; we have none, so derive stable cosmetic values from the id).
function acceptance(id: number): string {
  return (38 + ((id * 4099) % 480) / 10).toFixed(1);
}
function freqBars(id: number): number[] {
  return [0, 1, 2, 3, 4].map((i) => 0.28 + (((id >> i) & 3) / 3) * 0.72);
}

export function ProblemsBoard({ problems }: { problems: BoardItem[] }) {
  const solved = useSolved();
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<string | null>(null);
  const [difficulty, setDifficulty] = useState<Difficulty | 'All'>('All');

  const categories = useMemo(() => {
    const counts = new Map<string, number>();
    for (const p of problems)
      counts.set(p.category, (counts.get(p.category) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [problems]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return problems.filter((p) => {
      if (category && p.category !== category) return false;
      if (difficulty !== 'All' && p.difficulty !== difficulty) return false;
      if (q && !p.title.toLowerCase().includes(q) && !String(p.id).includes(q))
        return false;
      return true;
    });
  }, [problems, query, category, difficulty]);

  const solvedCount = problems.filter((p) => solved.has(p.slug)).length;
  const byDiff = (d: Difficulty) => {
    const total = problems.filter((p) => p.difficulty === d).length;
    const done = problems.filter(
      (p) => p.difficulty === d && solved.has(p.slug),
    ).length;
    return { done, total };
  };
  const pct = problems.length
    ? Math.round((solvedCount / problems.length) * 100)
    : 0;

  return (
    <div className="bc-board">
      <main className="bc-board-main">
        <div className="bc-chips">
          {categories.map(([cat, n]) => (
            <button
              type="button"
              key={cat}
              className={`bc-chip ${category === cat ? 'bc-chip-on' : ''}`}
              onClick={() => setCategory((c) => (c === cat ? null : cat))}
            >
              {cat}
              <span className="bc-chip-count">{n}</span>
            </button>
          ))}
        </div>

        <div className="bc-difftabs">
          {(['All', 'Easy', 'Medium', 'Hard'] as const).map((d) => (
            <button
              type="button"
              key={d}
              className={`bc-difftab ${difficulty === d ? 'bc-difftab-on' : ''}`}
              onClick={() => setDifficulty(d)}
            >
              {d}
            </button>
          ))}
        </div>

        <div className="bc-toolbar">
          <input
            className="bc-search font-mono"
            placeholder="Search questions"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <span className="bc-solved-inline font-mono">
            <span
              className="bc-solved-ring"
              style={{ '--pct': `${pct}%` } as CSSProperties}
            />
            {solvedCount}/{problems.length} Solved
          </span>
        </div>

        <ol className="bc-table">
          {filtered.map((p, i) => {
            const done = solved.has(p.slug);
            return (
              <li
                className={`bc-trow ${i % 2 ? 'bc-trow-alt' : ''}`}
                key={p.slug}
              >
                <Link className="bc-tlink" href={`/bamlcode/${p.slug}`}>
                  <span
                    className={`bc-tstatus ${done ? 'bc-tstatus-done' : ''}`}
                    aria-hidden
                  >
                    {done ? '✓' : ''}
                  </span>
                  <span className="bc-ttitle">
                    {p.id}. {p.title}
                  </span>
                  <span className="bc-tacc font-mono">{acceptance(p.id)}%</span>
                  <span className={`bc-tdiff ${DIFF_CLASS[p.difficulty]}`}>
                    {DIFF_ABBR[p.difficulty]}
                  </span>
                  <span className="bc-tfreq" aria-hidden>
                    {freqBars(p.id).map((h, bi) => (
                      <span
                        // biome-ignore lint/suspicious/noArrayIndexKey: static bars
                        key={bi}
                        className="bc-tfreq-bar"
                        style={{ height: `${Math.round(h * 100)}%` }}
                      />
                    ))}
                  </span>
                </Link>
              </li>
            );
          })}
          {filtered.length === 0 ? (
            <li className="bc-tempty font-mono">no problems match</li>
          ) : null}
        </ol>
      </main>

      <aside className="bc-rail">
        <div className="bc-progress-card">
          <div className="bc-progress-head font-mono">Progress</div>
          <div className="bc-progress-big">
            <span className="bc-progress-num">{solvedCount}</span>
            <span className="bc-progress-den">/ {problems.length} solved</span>
          </div>
          <div className="bc-progress-bar">
            <span style={{ width: `${pct}%` }} />
          </div>
          {(['Easy', 'Medium', 'Hard'] as const).map((d) => {
            const { done, total } = byDiff(d);
            if (total === 0) return null;
            return (
              <div className="bc-progress-row font-mono" key={d}>
                <span className={`bc-tdiff ${DIFF_CLASS[d]}`}>{d}</span>
                <span className="bc-progress-frac">
                  {done} / {total}
                </span>
              </div>
            );
          })}
        </div>
      </aside>
    </div>
  );
}
