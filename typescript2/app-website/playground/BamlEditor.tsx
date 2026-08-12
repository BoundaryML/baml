'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { codeToTokens, type ThemedToken } from 'shiki';
import { convertTextmateToShiki } from '@/lib/mdx/shiki-grammars';
import bamlTextmate from '@/lib/mdx/bamlTextmate.json';

interface BamlEditorProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  /** Hide the "main.baml" top strip. */
  chromeless?: boolean;
}

// ---------------------------------------------------------------------------
// Register BAML language with Shiki (once)
// ---------------------------------------------------------------------------

const bamlLang = convertTextmateToShiki(bamlTextmate as Record<string, any>);

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function BamlEditor({
  value,
  onChange,
  disabled,
  chromeless,
}: BamlEditorProps) {
  const [tokens, setTokens] = useState<ThemedToken[][] | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const highlightRef = useRef<HTMLDivElement>(null);

  // Tokenize with Shiki whenever value changes
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const result = await codeToTokens(value, {
          lang: bamlLang as any,
          theme: 'github-light',
        });
        if (!cancelled) setTokens(result.tokens);
      } catch {
        if (!cancelled) setTokens(null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [value]);

  // Sync scroll between textarea and the highlighted layer behind it
  const handleScroll = useCallback(() => {
    if (textareaRef.current && highlightRef.current) {
      highlightRef.current.scrollTop = textareaRef.current.scrollTop;
      highlightRef.current.scrollLeft = textareaRef.current.scrollLeft;
    }
  }, []);

  const highlightedLines = tokens
    ? tokens.map((line, li) => (
        <div key={li} className="min-h-[1.5rem]">
          {line.length === 0
            ? '​'
            : line.map((token, ti) => (
                <span key={ti} style={{ color: token.color }}>
                  {token.content}
                </span>
              ))}
        </div>
      ))
    : value.split('\n').map((line, i) => (
        <div key={i} className="min-h-[1.5rem]">
          {line || '​'}
        </div>
      ));

  return (
    // `nokey` opts the editor out of React Flow's global key capture
    // (the playground graph grabs Space/Backspace/etc otherwise).
    <div className="nokey relative flex flex-col h-full bg-[#FFFDF6]">
      {!chromeless && (
        <div className="flex items-center justify-between border-b border-border bg-muted/50 px-4 py-2">
          <span className="text-sm font-medium">main.baml</span>
        </div>
      )}

      <div className="flex-1 relative overflow-hidden">
        {/* Highlighted layer */}
        <div
          ref={highlightRef}
          className="absolute inset-0 p-4 font-mono text-sm leading-6 overflow-auto pointer-events-none whitespace-pre-wrap break-words"
          aria-hidden
        >
          {highlightedLines}
        </div>
        {/* Transparent textarea — captures all input */}
        <textarea
          ref={textareaRef}
          className="absolute inset-0 w-full h-full p-4 font-mono text-sm leading-6 bg-transparent text-transparent caret-[#6D28D9] resize-none outline-none overflow-auto whitespace-pre-wrap break-words"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onScroll={handleScroll}
          spellCheck={false}
          disabled={disabled}
        />
      </div>
    </div>
  );
}
