'use client';

import { Check, Copy } from 'lucide-react';
import { type ComponentProps, useRef } from 'react';

import { useCopyFeedback } from '@/components/use-copy-feedback';

export function CodeBlock({
  children,
  className,
  title,
  source,
  ...props
}: ComponentProps<'pre'> & { source?: string }) {
  const preRef = useRef<HTMLPreElement>(null);
  const { copiedAction, copy } = useCopyFeedback<'code'>();
  const copied = copiedAction === 'code';

  return (
    <figure className="docs-code not-typeset" dir="ltr">
      {title ? <figcaption>{title}</figcaption> : null}
      <button
        aria-label={copied ? 'Copied code' : 'Copy code'}
        className="docs-code-copy docs-focus-ring"
        onClick={() =>
          void copy('code', source ?? preRef.current?.textContent ?? '')
        }
        title={copied ? 'Copied!' : 'Copy code'}
        type="button"
      >
        {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      </button>
      <section
        aria-label={title ?? 'Code example'}
        className="docs-code-viewport"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: Focus enables keyboard scrolling of wide code.
        tabIndex={0}
      >
        <pre {...props} className={className} ref={preRef}>
          {children}
        </pre>
      </section>
    </figure>
  );
}
