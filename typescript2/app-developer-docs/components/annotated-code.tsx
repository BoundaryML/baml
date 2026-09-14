'use client';

import { Check, Copy, Maximize2 } from 'lucide-react';
import Image from 'next/image';
import { type ReactNode, useState } from 'react';
import { useCopyFeedback } from '@/components/use-copy-feedback';

export function AnnotatedCode({
  code,
  id,
  width,
  height,
  description,
  filename,
  children,
}: {
  code: string;
  id: string;
  width: number;
  height: number;
  description: string;
  filename?: string;
  children: ReactNode;
}) {
  const [showAnnotations, setShowAnnotations] = useState(true);
  const { copiedAction, copy } = useCopyFeedback<'code'>();
  const copied = copiedAction === 'code';
  return (
    <figure
      className="annotated-code not-typeset"
      data-annotation-id={id}
      dir="ltr"
    >
      {filename ? (
        <figcaption className="annotated-code-header">
          <span className="baml-code-tab">
            <Image
              alt="BAML"
              className="baml-code-mark"
              height={14}
              src="/baml-logo.png"
              width={14}
            />
            <span className="baml-code-filename" title={filename}>
              {filename.split('/').at(-1)}
            </span>
          </span>
        </figcaption>
      ) : null}
      <div className="annotated-code-actions">
        {showAnnotations &&
          (['light', 'dark'] as const).map((theme) => (
            <a
              aria-label="Open full-size annotated code"
              className={`annotated-${theme} docs-focus-ring`}
              href={`/book/annotations/${id}-${theme}.svg`}
              key={theme}
              rel="noreferrer"
              target="_blank"
              title="Open full size"
            >
              <Maximize2 aria-hidden="true" />
            </a>
          ))}
        <button
          aria-label={copied ? 'Copied code' : 'Copy code'}
          className="docs-focus-ring"
          onClick={() => void copy('code', code)}
          title={copied ? 'Copied!' : 'Copy code'}
          type="button"
        >
          {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
        </button>
      </div>
      <section
        aria-label={showAnnotations ? 'Annotated code example' : 'Code example'}
        className="annotated-code-viewport"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: Keyboard users can scroll long source lines when annotations are hidden.
        tabIndex={0}
      >
        {showAnnotations
          ? (['light', 'dark'] as const).map((theme) => (
              <Image
                alt={description}
                className={`annotated-code-image annotated-${theme}`}
                height={height}
                key={theme}
                src={`/book/annotations/${id}-${theme}.svg`}
                style={{
                  maxWidth: '100%',
                  width: width * 0.85,
                }}
                unoptimized
                width={width}
              />
            ))
          : children}
      </section>
      <div className="annotated-code-footer">
        <button
          className="annotated-code-toggle docs-focus-ring"
          onClick={() => setShowAnnotations((shown) => !shown)}
          type="button"
        >
          {showAnnotations ? 'Hide annotations' : 'Show annotations'}
        </button>
      </div>
    </figure>
  );
}
