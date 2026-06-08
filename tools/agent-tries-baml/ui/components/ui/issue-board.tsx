import { cva, type VariantProps } from 'class-variance-authority';
import Link from 'next/link';

import { cn } from '@/lib/utils';

/** The status-grouped column grid (legacy `.issueboard`). */
export function IssueBoard({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3.5 items-start">
      {children}
    </div>
  );
}

/** A flat (ungrouped) grid of issue-style cards — same card look, no columns. */
export function CardGrid({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3.5 items-stretch">
      {children}
    </div>
  );
}

/** One board column; the header carries a tone dot + count (legacy `.issuecol-head`). */
export function IssueColumn({
  head,
  children,
}: {
  head: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="text-xs font-semibold text-muted-foreground px-0.5 pt-1 pb-2 border-b border-border mb-2">
        {head}
      </div>
      {children}
    </div>
  );
}

/** One issue card linking to its issue page (legacy `.issuecard`). */
export function IssueCard({
  href,
  title,
  children,
  className,
  style,
}: {
  href: string;
  title?: string;
  className?: string;
  style?: React.CSSProperties;
  children: React.ReactNode;
}) {
  return (
    <Link
      href={href}
      title={title}
      style={style}
      className={cn(
        'block rounded-md border border-border px-2.5 py-2 mb-2 text-[13px] no-underline text-inherit hover:bg-muted',
        'transition-[transform,box-shadow] duration-150 hover:-translate-y-[2px] hover:shadow-[0_4px_10px_rgba(60,50,30,0.10)]',
        className,
      )}
    >
      {children}
    </Link>
  );
}

/** The card's title line (legacy `.issuecard-title`). */
export function IssueCardTitle({ children }: { children: React.ReactNode }) {
  return <span className="block mt-1 mb-0.5 leading-[1.35]">{children}</span>;
}

/** The card's muted meta line (legacy `.issuecard-meta`). */
export function IssueCardMeta({ children }: { children: React.ReactNode }) {
  return (
    <span className="font-mono text-[11.5px] text-muted-foreground">{children}</span>
  );
}

const kindTag = cva(
  'text-[10.5px] uppercase tracking-[0.04em] rounded px-[5px] py-px border',
  {
    variants: {
      kind: {
        default: 'text-muted-foreground border-border',
        language: 'text-destructive border-current',
        // run-outcome tones, so run cards share the issue-card tag style
        success: 'text-success border-current',
        failed: 'text-destructive border-current',
        partial: 'text-muted-foreground border-border',
      },
    },
    defaultVariants: { kind: 'default' },
  },
);

export type KindTagKind = VariantProps<typeof kindTag>['kind'];

const KIND_KEYS = new Set(['language', 'success', 'failed', 'partial']);

/** The kind/outcome tag on a card (legacy `.kindtag`); `language`/`failed` run hot. */
export function KindTag({
  kind,
  children,
}: {
  kind?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={kindTag({
        kind: kind && KIND_KEYS.has(kind) ? (kind as KindTagKind) : 'default',
      })}
    >
      {children}
    </span>
  );
}
