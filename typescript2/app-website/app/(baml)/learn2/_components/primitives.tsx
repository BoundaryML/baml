import type { ReactNode } from 'react';
import { cn } from '@/lib/utils';

/**
 * Presentational building blocks for slides. All are plain (server-compatible)
 * components — no hooks, no effects — so they can be composed inside the
 * server-rendered slide registry.
 */

/** Small uppercase mono eyebrow — the site's recurring kicker label. */
export function Kicker({ children }: { children: ReactNode }) {
  return (
    <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-[#8A8580]">
      {children}
    </p>
  );
}

/** The standard slide frame: kicker + big title + body, centered column. */
export function SlideShell({
  title,
  children,
  wide,
}: {
  /** Accepted for call-site compatibility but intentionally not rendered. */
  kicker?: string;
  title?: ReactNode;
  children?: ReactNode;
  wide?: boolean;
}) {
  return (
    <div className={cn('l2-slide-shell', wide && 'l2-slide-shell--wide')}>
      {title ? <h2 className="l2-slide-title">{title}</h2> : null}
      {children ? <div className="l2-slide-body">{children}</div> : null}
    </div>
  );
}

/** Large lead paragraph for a single strong statement. */
export function Lead({ children }: { children: ReactNode }) {
  return <p className="l2-lead">{children}</p>;
}

/** A pull-quote, used for the "observability" thesis slide. */
export function Quote({
  children,
  cite,
}: {
  children: ReactNode;
  cite?: string;
}) {
  return (
    <blockquote className="l2-quote">
      <p>{children}</p>
      {cite ? <cite className="l2-quote-cite">{cite}</cite> : null}
    </blockquote>
  );
}

export function Bullets({ items }: { items: ReactNode[] }) {
  return (
    <ul className="l2-bullets">
      {items.map((it, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static, order-stable content
        <li key={i}>{it}</li>
      ))}
    </ul>
  );
}

/** Two-pane layout: prose on the left, code/visual on the right. */
export function Split({ left, right }: { left: ReactNode; right: ReactNode }) {
  return (
    <div className="l2-split">
      <div className="l2-split-left">{left}</div>
      <div className="l2-split-right">{right}</div>
    </div>
  );
}

/** A full-bleed section divider ("Section 3 · Parallelism"). */
export function SectionDivider({
  index,
  title,
  blurb,
}: {
  index: string;
  title: string;
  blurb?: string;
}) {
  return (
    <div className="l2-section-divider">
      <span className="l2-section-index font-mono">{index}</span>
      <h2 className="l2-section-title">{title}</h2>
      {blurb ? <p className="l2-section-blurb">{blurb}</p> : null}
    </div>
  );
}

type CalloutTone = 'note' | 'warn' | 'good';

/** A small inline callout. `warn` is used to flag aspirational/not-yet-shipped syntax. */
export function Callout({
  tone = 'note',
  children,
}: {
  tone?: CalloutTone;
  children: ReactNode;
}) {
  return (
    <div className={cn('l2-callout', `l2-callout--${tone}`)}>{children}</div>
  );
}

/** A keycap-style chip (e.g. for keyboard hints). */
export function Key({ children }: { children: ReactNode }) {
  return <kbd className="l2-key">{children}</kbd>;
}

/** A dark terminal box. Each line is prefixed with a `$` prompt. */
export function Terminal({ lines }: { lines: string[] }) {
  return (
    <div className="l2-terminal">
      <div className="l2-terminal-bar" aria-hidden>
        <i />
        <i />
        <i />
      </div>
      <pre className="l2-terminal-body">
        {lines.map((line, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static, order-stable
          <div key={i} className="l2-terminal-line">
            <span className="l2-terminal-prompt">$</span> {line}
          </div>
        ))}
      </pre>
    </div>
  );
}
