import type { ReactNode } from 'react';

// Highlights BAML one line at a time, in the page's two brand colors and white. Every sample on the
// page compiles with baml check.
const KEYWORDS =
  'function|class|enum|interface|implements|extends|type|let|for|in|if|else|while|match|return|await|spawn|throw|throws|try|catch|break|continue|test|testset|client|tools|prompt|self|null|true|false';
const TYPES = 'string|float|int|bigint|bool|void|never|unknown|map|json';

// In order: comments, strings, keywords, built-in types, numbers, type names, and function calls
// (including generic ones, `eval<int>(`).
const TOKENS = new RegExp(
  `(//.*$|"[^"]*"|\`[^\`]*\`|\\b(?:${KEYWORDS})\\b|\\b(?:${TYPES})\\b|\\b\\d+(?:\\.\\d+)?\\b|\\b[A-Z]\\w*\\b|\\b[a-z_]\\w*(?=\\(|<))`,
);

const KEYWORD = new RegExp(`^(?:${KEYWORDS})$`);
const STRING = 'text-(--color-pasture-200)';

// Keywords in the brand purple; types, type names and function calls in white, like the TypeScript
// listings beside them; strings and numbers in green; comments dimmed, doc comments a little less,
// since the model reads them.
function tone(part: string): string | undefined {
  if (part.startsWith('///')) return 'text-white/50';
  if (part.startsWith('//')) return 'text-white/35';
  if (part.startsWith('"') || /^\d/.test(part)) return STRING;
  if (KEYWORD.test(part)) return 'text-clover';
  return 'text-white';
}

export function Highlight({ line }: { line: string }): ReactNode {
  return line.split(TOKENS).map((part, i) => {
    const key = `${i}-${part}`;
    if (i % 2 === 0) return part;
    // A template string keeps its ${...} in white, as a prompt's lines do.
    if (part.startsWith('`')) return <Prose key={key} line={part} />;
    return (
      <span className={tone(part)} key={key}>
        {part}
      </span>
    );
  });
}

export type LineMark = 'added' | 'removed' | 'running';

const MARK: Record<LineMark, string> = {
  added: 'bg-(--color-pasture-200)/12',
  removed: 'bg-[#E5484D]/14',
  running: 'bg-clover/12',
};

/** Numbered source lines, with an optional mark on any of them. Lines listed in prose are a prompt's text.
 *  With `wrap`, a line too long for the panel continues on the next row, indented past the line's own
 *  indent, instead of scrolling. */
export function Code({
  focusable = true,
  lines,
  marks = {},
  notes = {},
  prose = [],
  wrap = false,
}: {
  /** False while part of the code is clipped, so the keyboard can't reach a noted line out of view. */
  focusable?: boolean;
  lines: string[];
  marks?: Record<number, LineMark>;
  /** A short explanation for a line, shown when it's hovered or focused. */
  notes?: Record<number, string>;
  prose?: number[];
  /** `md` wraps only from tablet width up; on a phone the line keeps its shape and the panel scrolls
   *  sideways, so code is never broken mid-word. */
  wrap?: boolean | 'md';
}) {
  return (
    <div
      className={`font-mono text-[12.5px] md:text-[14px] leading-[22px] md:leading-6 text-[#BDBDC4] ${wrap === 'md' ? 'w-max min-w-full md:w-auto' : ''}`}
    >
      {lines.map((line, i) => (
        <div
          className={`flex pr-4 transition-colors duration-200 ${marks[i] ? MARK[marks[i]] : ''} ${notes[i] ? 'group/note relative cursor-help focus-ring outline-none' : ''}`}
          data-line={i}
          // biome-ignore lint/suspicious/noArrayIndexKey: a sample's lines keep their positions
          key={i}
          tabIndex={notes[i] && focusable ? 0 : undefined}
        >
          <span className="shrink-0 w-10 md:w-12 pr-4 text-right text-white/20 select-none">
            {i + 1}
          </span>
          <span
            className={`min-w-0 ${wrap === 'md' ? 'whitespace-pre md:whitespace-pre-wrap md:[overflow-wrap:anywhere]' : wrap ? 'whitespace-pre-wrap [overflow-wrap:anywhere]' : 'whitespace-pre'}`}
            style={wrap ? hang(line) : undefined}
          >
            {prose.includes(i) ? (
              <span className="text-(--color-pasture-200)">{line}</span>
            ) : notes[i] ? (
              // The underline starts at the code, not under the indentation before it.
              <>
                {line.slice(0, line.length - line.trimStart().length)}
                <span className="underline decoration-dotted decoration-white/35 underline-offset-[5px]">
                  <Highlight line={line.trimStart()} />
                </span>
              </>
            ) : (
              <Highlight line={line} />
            )}
          </span>
          {notes[i] && (
            // Below the first lines and above the rest, so the panel's edges don't cut it off.
            <span
              className={`pointer-events-none absolute left-10 md:left-12 z-20 w-max max-w-[min(26rem,calc(100%-4rem))] rounded-lg border border-white/15 bg-[#0b0712] px-3 py-2 font-sans text-[13px] leading-snug text-white/90 opacity-0 shadow-lg transition-opacity duration-150 [text-indent:0] [white-space:normal] group-hover/note:opacity-100 group-focus/note:opacity-100 ${i < 4 ? 'top-full mt-1' : 'bottom-full mb-1'}`}
              role="tooltip"
            >
              {notes[i]}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

/** A hanging indent for a wrapped line: its first row starts at the left edge as usual, and the rows
 *  after it start two characters past the line's leading spaces. */
function hang(line: string) {
  const indent = `${line.length - line.trimStart().length + 2}ch`;
  return { paddingLeft: indent, textIndent: `-${indent}` };
}

/** The text inside a multi-line prompt: prose in the string color, with each ${...} in white. */
export function Prose({ line }: { line: string }): ReactNode {
  return line.split(/(\$\{[\w.]+\})/).map((part, i) => (
    <span
      className={i % 2 ? 'text-white' : 'text-(--color-pasture-200)'}
      // biome-ignore lint/suspicious/noArrayIndexKey: a line's parts keep their positions
      key={i}
    >
      {part}
    </span>
  ));
}

/** Which lines sit inside a multi-line prompt, between `prompt: \`` and the closing backtick. */
export function proseLines(lines: string[]): Set<number> {
  const prose = new Set<number>();
  let open = false;
  lines.forEach((line, i) => {
    if (open && line.trim() === '`') open = false;
    else if (open) prose.add(i);
    else if (line.trimEnd().endsWith('prompt: `')) open = true;
  });
  return prose;
}
