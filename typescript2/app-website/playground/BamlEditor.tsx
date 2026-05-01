'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { codeToTokens, type ThemedToken } from 'shiki';
import { convertTextmateToShiki } from '@/lib/mdx/shiki-grammars';
import bamlTextmate from '@/lib/mdx/bamlTextmate.json';

interface BamlEditorProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  /** Hide the "main.baml" top strip; the toggle button still floats top-right. */
  chromeless?: boolean;
}

// ---------------------------------------------------------------------------
// Register BAML language with Shiki (once)
// ---------------------------------------------------------------------------

const bamlLang = convertTextmateToShiki(bamlTextmate as Record<string, any>);

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function BamlEditor({ value, onChange, disabled, chromeless }: BamlEditorProps) {
  const [trainingWheels, setTrainingWheels] = useState(true);
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
          theme: 'github-dark',
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
          {line.length === 0 ? (
            '​'
          ) : (
            line.map((token, ti) => (
              <span key={ti} style={{ color: token.color }}>
                {token.content}
              </span>
            ))
          )}
        </div>
      ))
    : value.split('\n').map((line, i) => (
        <div key={i} className="min-h-[1.5rem]">
          {line || '​'}
        </div>
      ));

  return (
    <div className="relative flex flex-col h-full bg-[#0d1117]">
      {!chromeless && (
        <div className="flex items-center justify-between border-b border-border bg-muted/50 px-4 py-2">
          <span className="text-sm font-medium">main.baml</span>
        </div>
      )}

      {/* Floating training-wheels toggle — always visible regardless of chromeless */}
      <button
        type="button"
        className={`absolute top-2 right-2 z-10 text-[11px] px-2 py-1 rounded border transition-colors ${
          trainingWheels
            ? 'bg-purple-500/10 text-purple-300 hover:bg-purple-500/20 border-purple-500/30'
            : 'bg-emerald-500/10 text-emerald-300 hover:bg-emerald-500/20 border-emerald-500/30'
        }`}
        onClick={() => setTrainingWheels((prev) => !prev)}
      >
        {trainingWheels ? 'Remove Training Wheels' : 'Re-enable Training Wheels'}
      </button>

      {trainingWheels ? (
        // Read-only highlighted view
        <div
          className="flex-1 overflow-auto p-4 font-mono text-sm leading-6 whitespace-pre-wrap break-words select-text cursor-default"
          aria-label="BAML source (read-only)"
        >
          {highlightedLines}
        </div>
      ) : (
        // Full edit mode: transparent textarea over highlighted layer
        <div className="flex-1 relative overflow-hidden">
          <div
            ref={highlightRef}
            className="absolute inset-0 p-4 font-mono text-sm leading-6 overflow-auto pointer-events-none whitespace-pre-wrap break-words"
            aria-hidden
          >
            {highlightedLines}
          </div>
          <textarea
            ref={textareaRef}
            className="absolute inset-0 w-full h-full p-4 font-mono text-sm leading-6 bg-transparent text-transparent caret-white resize-none outline-none overflow-auto whitespace-pre-wrap break-words"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onScroll={handleScroll}
            spellCheck={false}
            disabled={disabled}
          />
        </div>
      )}
    </div>
  );
}
