'use client';

import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { codeToTokens, type ThemedToken } from 'shiki';
import { convertTextmateToShiki } from '@/lib/mdx/shiki-grammars';
import bamlTextmate from '@/lib/mdx/bamlTextmate.json';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EditableRegion {
  /** Byte offset into the code string */
  start: number;
  end: number;
  kind: 'prompt' | 'function_name' | 'test_arg' | 'enum_value';
}

interface BamlEditorProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

// ---------------------------------------------------------------------------
// Parse editable regions from BAML source
// ---------------------------------------------------------------------------

function findEditableRegions(code: string): EditableRegion[] {
  const regions: EditableRegion[] = [];

  // Find prompt blocks: #"..."# (content between the delimiters)
  const promptRegex = /#"\n?([\s\S]*?)\n?\s*"#/g;
  let m: RegExpExecArray | null;
  while ((m = promptRegex.exec(code)) !== null) {
    const contentStart = m.index + m[0].indexOf(m[1]!);
    regions.push({
      start: contentStart,
      end: contentStart + m[1]!.length,
      kind: 'prompt',
    });
  }

  // Find function names: function <Name>(
  const fnRegex = /\bfunction\s+(\w+)\s*\(/g;
  while ((m = fnRegex.exec(code)) !== null) {
    const nameStart = m.index + m[0].indexOf(m[1]!);
    regions.push({
      start: nameStart,
      end: nameStart + m[1]!.length,
      kind: 'function_name',
    });
  }

  // Find test args blocks: args { ... }
  const testArgsRegex = /\bargs\s*\{([^}]*)\}/g;
  while ((m = testArgsRegex.exec(code)) !== null) {
    const contentStart = m.index + m[0].indexOf(m[1]!);
    regions.push({
      start: contentStart,
      end: contentStart + m[1]!.length,
      kind: 'test_arg',
    });
  }

  // Find enum values (lines inside enum blocks)
  const enumRegex = /\benum\s+\w+\s*\{([^}]*)\}/g;
  while ((m = enumRegex.exec(code)) !== null) {
    const body = m[1]!;
    const bodyStart = m.index + m[0].indexOf(body);
    const valueRegex = /^\s*(\w+)\s*$/gm;
    let vm: RegExpExecArray | null;
    while ((vm = valueRegex.exec(body)) !== null) {
      const valStart = bodyStart + vm.index + vm[0].indexOf(vm[1]!);
      regions.push({
        start: valStart,
        end: valStart + vm[1]!.length,
        kind: 'enum_value',
      });
    }
  }

  return regions.sort((a, b) => a.start - b.start);
}

// ---------------------------------------------------------------------------
// Register BAML language with Shiki (once)
// ---------------------------------------------------------------------------

const bamlLang = convertTextmateToShiki(bamlTextmate as Record<string, any>);

// Map Shiki scope names to Tailwind-ish colors (dark theme)
function scopeToColor(scopes: string[]): string {
  const scope = scopes[scopes.length - 1] ?? '';
  if (scope.startsWith('comment')) return '#6a737d';
  if (scope.startsWith('keyword') || scope.startsWith('storage.type')) return '#c586c0';
  if (scope.startsWith('entity.name.function')) return '#dcdcaa';
  if (scope.startsWith('entity.name.type')) return '#4ec9b0';
  if (scope.startsWith('string')) return '#ce9178';
  if (scope.startsWith('constant.numeric')) return '#b5cea8';
  if (scope.startsWith('constant.language')) return '#569cd6';
  if (scope.startsWith('variable')) return '#9cdcfe';
  if (scope.startsWith('support.type')) return '#4ec9b0';
  if (scope.startsWith('punctuation')) return '#d4d4d4';
  return '#d4d4d4';
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function BamlEditor({ value, onChange, disabled }: BamlEditorProps) {
  const [trainingWheels, setTrainingWheels] = useState(true);
  const [tokens, setTokens] = useState<ThemedToken[][] | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const highlightRef = useRef<HTMLDivElement>(null);

  // Tokenize with Shiki
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
    return () => { cancelled = true; };
  }, [value]);

  // Sync scroll between textarea and highlighted layer
  const handleScroll = useCallback(() => {
    if (textareaRef.current && highlightRef.current) {
      highlightRef.current.scrollTop = textareaRef.current.scrollTop;
      highlightRef.current.scrollLeft = textareaRef.current.scrollLeft;
    }
  }, []);

  // Editable regions for training wheels mode
  const editableRegions = useMemo(() => {
    if (!trainingWheels) return [];
    return findEditableRegions(value);
  }, [value, trainingWheels]);

  // Handle inline edits in training wheels mode
  const handleRegionEdit = useCallback(
    (region: EditableRegion, newText: string) => {
      const before = value.slice(0, region.start);
      const after = value.slice(region.end);
      onChange(before + newText + after);
    },
    [value, onChange],
  );

  // ---------------------------------------------------------------------------
  // Training Wheels mode: render highlighted code with editable spans
  // ---------------------------------------------------------------------------
  if (trainingWheels) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between border-b border-border bg-muted/50 px-4 py-2">
          <span className="text-sm font-medium">main.baml</span>
          <button
            className="text-xs px-2 py-1 rounded bg-purple-500/10 text-purple-400 hover:bg-purple-500/20 transition-colors border border-purple-500/20"
            onClick={() => setTrainingWheels(false)}
          >
            Disable Training Wheels
          </button>
        </div>
        <div className="flex-1 overflow-auto p-4 font-mono text-sm leading-6 bg-[#0d1117] whitespace-pre-wrap">
          <TrainingWheelsView
            code={value}
            tokens={tokens}
            regions={editableRegions}
            onRegionEdit={handleRegionEdit}
            disabled={disabled}
          />
        </div>
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Full edit mode: transparent textarea over syntax-highlighted code
  // ---------------------------------------------------------------------------
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between border-b border-border bg-muted/50 px-4 py-2">
        <span className="text-sm font-medium">main.baml</span>
        <button
          className="text-xs px-2 py-1 rounded bg-green-500/10 text-green-400 hover:bg-green-500/20 transition-colors border border-green-500/20"
          onClick={() => setTrainingWheels(true)}
        >
          Enable Training Wheels
        </button>
      </div>
      <div className="flex-1 relative overflow-hidden bg-[#0d1117]">
        {/* Highlighted layer */}
        <div
          ref={highlightRef}
          className="absolute inset-0 p-4 font-mono text-sm leading-6 overflow-auto pointer-events-none whitespace-pre-wrap break-words"
          aria-hidden
        >
          {tokens
            ? tokens.map((line, li) => (
                <div key={li} className="min-h-[1.5rem]">
                  {line.map((token, ti) => (
                    <span key={ti} style={{ color: token.color }}>
                      {token.content}
                    </span>
                  ))}
                </div>
              ))
            : value.split('\n').map((line, i) => (
                <div key={i} className="min-h-[1.5rem]">{line}</div>
              ))}
        </div>
        {/* Transparent textarea */}
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
    </div>
  );
}

// ---------------------------------------------------------------------------
// Training Wheels View: renders syntax-highlighted code with editable regions
// ---------------------------------------------------------------------------

function TrainingWheelsView({
  code,
  tokens,
  regions,
  onRegionEdit,
  disabled,
}: {
  code: string;
  tokens: ThemedToken[][] | null;
  regions: EditableRegion[];
  onRegionEdit: (region: EditableRegion, newText: string) => void;
  disabled?: boolean;
}) {
  // Build a flat colored-character map, then overlay editable regions
  // We work line by line, char by char.

  const lines = code.split('\n');
  let charOffset = 0;

  // Precompute per-character colors from tokens
  const charColors: string[] = new Array(code.length).fill('#d4d4d4');
  if (tokens) {
    let offset = 0;
    for (const line of tokens) {
      for (const token of line) {
        const color = token.color ?? '#d4d4d4';
        for (let i = 0; i < token.content.length; i++) {
          if (offset + i < charColors.length) {
            charColors[offset + i] = color;
          }
        }
        offset += token.content.length;
      }
      offset++; // newline
    }
  }

  // Check if a character position falls within an editable region
  function getRegion(pos: number): EditableRegion | undefined {
    return regions.find((r) => pos >= r.start && pos < r.end);
  }

  const renderedLines: React.ReactNode[] = [];
  charOffset = 0;

  for (let li = 0; li < lines.length; li++) {
    const line = lines[li]!;
    const spans: React.ReactNode[] = [];
    let i = 0;

    while (i < line.length) {
      const absPos = charOffset + i;
      const region = getRegion(absPos);

      if (region) {
        // Render the editable region
        const regionStartInLine = region.start - charOffset;
        const regionEndInLine = Math.min(region.end - charOffset, line.length);
        const regionText = line.slice(regionStartInLine, regionEndInLine);

        spans.push(
          <EditableSpan
            key={`edit-${absPos}`}
            text={regionText}
            region={region}
            code={code}
            charOffset={charOffset}
            onEdit={onRegionEdit}
            disabled={disabled}
          />,
        );
        i = regionEndInLine;
      } else {
        // Render non-editable characters with their color, batching same-color runs
        const color = charColors[absPos] ?? '#d4d4d4';
        let end = i + 1;
        while (
          end < line.length &&
          !getRegion(charOffset + end) &&
          (charColors[charOffset + end] ?? '#d4d4d4') === color
        ) {
          end++;
        }
        spans.push(
          <span key={`t-${absPos}`} style={{ color }} className="select-none">
            {line.slice(i, end)}
          </span>,
        );
        i = end;
      }
    }

    renderedLines.push(
      <div key={li} className="min-h-[1.5rem]">
        {spans.length > 0 ? spans : '\u200B'}
      </div>,
    );
    charOffset += line.length + 1; // +1 for newline
  }

  return <>{renderedLines}</>;
}

// ---------------------------------------------------------------------------
// Editable inline span for training wheels regions
// ---------------------------------------------------------------------------

function EditableSpan({
  text,
  region,
  code,
  charOffset,
  onEdit,
  disabled,
}: {
  text: string;
  region: EditableRegion;
  code: string;
  charOffset: number;
  onEdit: (region: EditableRegion, newText: string) => void;
  disabled?: boolean;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const [focused, setFocused] = useState(false);

  // Get the full region text across lines
  const fullRegionText = code.slice(region.start, region.end);

  // For multi-line regions (prompts), we render only our line portion
  // but on edit we need to reconstruct the full region text
  const handleInput = useCallback(() => {
    if (!ref.current) return;
    const newLineText = ref.current.textContent ?? '';

    // Calculate which part of the full region this line covers
    const lineRegionStart = Math.max(0, charOffset - region.start);
    const lineRegionEnd = lineRegionStart + text.length;

    const newFullText =
      fullRegionText.slice(0, lineRegionStart) +
      newLineText +
      fullRegionText.slice(lineRegionEnd);

    onEdit(region, newFullText);
  }, [text, region, fullRegionText, charOffset, onEdit]);

  const bgColor = focused
    ? 'rgba(59, 130, 246, 0.15)'
    : 'rgba(59, 130, 246, 0.06)';

  const borderColor = focused
    ? 'rgba(59, 130, 246, 0.4)'
    : 'rgba(59, 130, 246, 0.15)';

  return (
    <span
      ref={ref}
      contentEditable={!disabled}
      suppressContentEditableWarning
      onFocus={() => setFocused(true)}
      onBlur={() => {
        setFocused(false);
        handleInput();
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && region.kind !== 'prompt') {
          e.preventDefault();
        }
      }}
      className="outline-none rounded-sm transition-colors"
      style={{
        backgroundColor: bgColor,
        borderBottom: `1px solid ${borderColor}`,
        color: region.kind === 'prompt' ? '#ce9178' : region.kind === 'function_name' ? '#dcdcaa' : '#d4d4d4',
        padding: '0 1px',
        cursor: disabled ? 'default' : 'text',
      }}
    >
      {text}
    </span>
  );
}
