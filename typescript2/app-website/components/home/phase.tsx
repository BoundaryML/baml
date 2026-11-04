import type { ReactNode } from 'react';
import { section } from './analytics';

// The four sections after the video take the rail's phases in turn: Build, Deploy, Observe, and
// Improve. Each has the phase's name over its heading, drawn like a step on the rail, then a short
// intro, one visual, and usually a few points.

export function Phase({
  children,
  heading,
  id,
  intro,
  name,
  soon = false,
}: {
  children: ReactNode;
  heading: ReactNode;
  id: string;
  intro: ReactNode;
  name: string;
  // Marks a phase whose features haven't shipped yet.
  soon?: boolean;
}) {
  return (
    <section
      {...section(id)}
      aria-labelledby={id}
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <p className="v4-caption flex items-center gap-2.5 text-ink-2">
          <span
            aria-hidden
            className="size-[11px] rounded-full border border-clover bg-clover"
          />
          {name}
          {soon && (
            <span className="ml-1 rounded-full border border-clover/40 px-2.5 py-1 text-clover">
              Coming soon
            </span>
          )}
        </p>
        <h2 className="v4-h2 mt-4 max-w-[22em]" id={id}>
          {heading}
        </h2>
        <p className="mt-5 max-w-[40rem] v4-body text-ink-2">{intro}</p>
        <div className="mt-10 md:mt-14">{children}</div>
      </div>
    </section>
  );
}

export type Point = { body: ReactNode; command?: string; title: string };

/** A phase's points: an optional command, a short title, and a sentence or two. */
export function Points({
  columns = 1,
  points,
}: {
  columns?: 1 | 2 | 3 | 4;
  points: Point[];
}) {
  const grid = {
    1: '',
    2: 'sm:grid-cols-2',
    3: 'sm:grid-cols-2 lg:grid-cols-3',
    4: 'sm:grid-cols-2 lg:grid-cols-4',
  }[columns];
  return (
    <dl className={`grid gap-x-8 gap-y-7 ${grid}`}>
      {points.map((p) => (
        <div key={p.title}>
          {p.command && (
            <p className="mb-1.5 font-mono text-[13px] text-clover">
              {p.command}
            </p>
          )}
          <dt className="v4-body text-head">{p.title}</dt>
          <dd className="mt-1 v4-body text-body">{p.body}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A command or identifier inside prose. */
export function Cmd({ children }: { children: ReactNode }) {
  return <code className="font-mono text-[0.88em] text-ink">{children}</code>;
}

/** A code or terminal panel in the page's dark editor style. */
export function Panel({
  children,
  className = '',
  title,
}: {
  children: ReactNode;
  className?: string;
  title: string;
}) {
  return (
    <figure
      className={`rounded-xl overflow-hidden border border-rule bg-raised ${className}`}
    >
      <figcaption className="v4-caption h-10 flex items-center px-4 border-b border-rule bg-raised text-muted">
        {title}
      </figcaption>
      {children}
    </figure>
  );
}
