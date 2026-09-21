import { ChevronRight } from 'lucide-react';
import { Children, cloneElement, isValidElement, type ReactNode } from 'react';

const treatments = {
  added: {
    description: 'BAML adds a capability.',
    label: 'Added',
    symbol: '+',
  },
  modified: {
    description: 'A familiar concept with different rules.',
    label: 'Modified',
    symbol: '~',
  },
  removed: {
    description: 'Syntax from your language that BAML doesn’t support.',
    label: 'Removed',
    symbol: '−',
  },
  same: {
    description: 'Already familiar. Expand for examples.',
    label: 'Same',
    symbol: '=',
  },
} as const;

function PerspectiveBadge({ kind }: { kind: keyof typeof treatments }) {
  return (
    <span className="perspective-note-badge">
      <span aria-hidden="true">{treatments[kind].symbol}</span>
      {treatments[kind].label}
    </span>
  );
}

/** Keep a section's classification beside its normal MDX heading. */
export function PerspectiveSection({
  children,
  kind,
}: {
  children: ReactNode;
  kind: Exclude<keyof typeof treatments, 'same'>;
}) {
  const [heading, ...body] = Children.toArray(children);
  if (!isValidElement<{ children?: ReactNode; className?: string }>(heading))
    throw new Error('PerspectiveSection must start with an MDX heading');
  return (
    <section className="perspective-section" data-kind={kind}>
      {cloneElement(heading, {
        children: (
          <>
            {heading.props.children}
            <PerspectiveBadge kind={kind} />
          </>
        ),
        className: `${heading.props.className ?? ''} perspective-section-heading`,
      })}
      {body}
    </section>
  );
}

/** A compact key for readers entering a language-specific perspective. */
export function PerspectiveKey() {
  return (
    <aside
      aria-label="How to read this perspective"
      className="perspective-key not-typeset"
    >
      {(['same', 'added', 'removed', 'modified'] as const).map((kind) => (
        <div
          className="perspective-key-item"
          data-kind={kind}
          key={kind}
          title={treatments[kind].description}
        >
          <PerspectiveBadge kind={kind} />
          <span className="sr-only">{treatments[kind].description}</span>
        </div>
      ))}
    </aside>
  );
}

/** Familiar material stays available without competing with the differences. */
export function PerspectiveNote({
  children,
  kind,
  summary,
}: {
  children: ReactNode;
  kind: keyof typeof treatments;
  summary: string;
}) {
  const heading = (
    <>
      <PerspectiveBadge kind={kind} />
      <span className="perspective-note-summary">{summary}</span>
    </>
  );
  if (kind === 'same')
    return (
      <details className="perspective-note" data-kind={kind}>
        <summary className="perspective-note-header">
          {heading}
          <ChevronRight
            aria-hidden="true"
            className="perspective-note-chevron"
            size={14}
          />
        </summary>
        <div className="perspective-note-body">{children}</div>
      </details>
    );
  return (
    <div className="perspective-note" data-kind={kind}>
      <div className="perspective-note-header">{heading}</div>
      <div className="perspective-note-body">{children}</div>
    </div>
  );
}
