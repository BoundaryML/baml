'use client';

import { Check, Copy, Link2 } from 'lucide-react';
import { useCopyFeedback } from '@/components/use-copy-feedback';

type CopyAction = 'declaration' | 'link';

export function GeneratedMemberActions({
  anchor,
  declaration,
  label,
}: {
  anchor: string;
  declaration: string;
  label: string;
}) {
  const { copiedAction, copy } = useCopyFeedback<CopyAction>();

  const copyLink = () => {
    const url = new URL(window.location.href);
    url.hash = anchor;
    void copy('link', url.toString());
  };

  const buttonClass =
    'docs-focus-ring inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground';

  return (
    <div className="absolute top-2 right-2 flex items-center rounded-md border bg-background/95 p-0.5 opacity-100 shadow-sm transition-opacity sm:opacity-0 sm:group-hover/member:opacity-100 sm:group-focus-within/member:opacity-100">
      <button
        aria-label={`Copy link to ${label}`}
        className={buttonClass}
        onClick={copyLink}
        title="Copy link"
        type="button"
      >
        {copiedAction === 'link' ? (
          <Check aria-hidden="true" className="size-3.5" />
        ) : (
          <Link2 aria-hidden="true" className="size-3.5" />
        )}
      </button>
      <button
        aria-label={`Copy declaration for ${label}`}
        className={buttonClass}
        onClick={() => void copy('declaration', declaration)}
        title="Copy declaration"
        type="button"
      >
        {copiedAction === 'declaration' ? (
          <Check aria-hidden="true" className="size-3.5" />
        ) : (
          <Copy aria-hidden="true" className="size-3.5" />
        )}
      </button>
    </div>
  );
}
