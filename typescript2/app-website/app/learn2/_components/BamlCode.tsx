'use client';

import { type CSSProperties, use } from 'react';
import type { BundledLanguage } from 'shiki';
import { cn } from '@/lib/utils';
import { useCodeTheme } from '../_lib/code-theme';
import { getLearnHighlighter } from '../_lib/highlighter';
import type { BamlCodeProps, Diagnostic, Severity } from '../_lib/types';

const SEV_CLASS: Record<Severity, string> = {
  error: 'l2-sev-error',
  warning: 'l2-sev-warning',
  info: 'l2-sev-info',
};

// Shiki FontStyle bitmask: Italic=1, Bold=2, Underline=4. Italic is
// deliberately ignored — github-light italicizes comments, and in these
// decks italics are reserved for inline diagnostics.
function tokenStyle(color?: string, fontStyle?: number): CSSProperties {
  const style: CSSProperties = {};
  if (color) style.color = color;
  if (fontStyle) {
    if (fontStyle & 2) style.fontWeight = 600;
    if (fontStyle & 4) style.textDecoration = 'underline';
  }
  return style;
}

/**
 * Client Component. Highlights BAML (and Python/TS/etc.) with Shiki and renders
 * line-by-line so we can overlay:
 *  - Error-Lens style inline diagnostics at end-of-line, and
 *  - margin "notes" that point at a specific line.
 *
 * Reads the shared highlighter via `use()` (the promise is memoised, so this
 * suspends once and is instant thereafter) — keeps the deck fully client-side
 * without a `useEffect`. Read-only by design; the editable Monaco variant is
 * `BamlEditor`.
 */
export function BamlCode({
  code,
  lang = 'baml',
  filename,
  diagnostics = [],
  notes = [],
  highlightLines = [],
  startLine = 1,
  noLineNumbers = false,
  wrap = false,
}: BamlCodeProps) {
  const theme = useCodeTheme();
  const highlighter = use(getLearnHighlighter());
  const { tokens } = highlighter.codeToTokens(code.replace(/\n+$/, ''), {
    // `baml`/`baml-jinja` are registered at runtime but aren't in Shiki's
    // BundledLanguage literal union; the cast is safe given the registration.
    lang: lang as BundledLanguage,
    theme: theme.shiki,
    // light themes paint keywords red (#cf222e); in the deck, red belongs to
    // diagnostics only, so remap keyword red to blue. Dark themes don't need it.
    colorReplacements: theme.shikiKeywordRemap,
  });

  const diagByLine = new Map<number, Diagnostic>();
  for (const d of diagnostics) diagByLine.set(d.line, d);
  const noteByLine = new Map<number, string>();
  for (const n of notes) noteByLine.set(n.line, n.text);
  const hlSet = new Set(highlightLines);

  return (
    <figure className={cn(`l2-code l2-code--${lang}`, wrap && 'l2-code--wrap')}>
      {filename ? (
        <figcaption
          className={cn(
            'l2-code-head',
            filename.toLowerCase().endsWith('.baml') && 'l2-code-head--baml',
          )}
        >
          <span className="l2-code-dots" aria-hidden>
            <i />
            <i />
            <i />
          </span>
          <span className="l2-code-name font-mono">{filename}</span>
        </figcaption>
      ) : null}
      <div className="l2-code-scroll">
        <pre className="l2-pre">
          <code>
            {tokens.map((line, i) => {
              const lineNo = i + startLine;
              const diag = diagByLine.get(lineNo);
              const note = noteByLine.get(lineNo);
              return (
                <div
                  // biome-ignore lint/suspicious/noArrayIndexKey: lines are order-stable
                  key={i}
                  className={cn(
                    'l2-line',
                    hlSet.has(lineNo) && 'l2-line-hl',
                    diag && `l2-line-diag ${SEV_CLASS[diag.severity]}`,
                  )}
                >
                  {noLineNumbers ? null : (
                    <span className="l2-ln" aria-hidden>
                      {lineNo}
                    </span>
                  )}
                  <span className="l2-lc">
                    {line.length === 0 ? (
                      '​'
                    ) : (
                      <>
                        {line.map((t, j) => (
                          <span
                            // biome-ignore lint/suspicious/noArrayIndexKey: tokens are order-stable
                            key={j}
                            style={tokenStyle(t.color, t.fontStyle)}
                          >
                            {t.content}
                          </span>
                        ))}
                      </>
                    )}
                    {diag ? (
                      <span
                        className={cn('l2-errorlens', SEV_CLASS[diag.severity])}
                      >
                        {diag.message}
                      </span>
                    ) : null}
                  </span>
                  {note ? (
                    <span className="l2-note">
                      <span className="l2-note-arrow" aria-hidden>
                        ←
                      </span>
                      {note}
                    </span>
                  ) : null}
                </div>
              );
            })}
          </code>
        </pre>
      </div>
    </figure>
  );
}
