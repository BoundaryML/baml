import type { ReactNode } from 'react';

// Highlights BAML one line at a time, in the page's two brand colors and white. Every sample on this
// hero compiles with baml check.
const TOKENS =
  /(\/\/.*$|"[^"]*"|`[^`]*`|\b(?:function|class|type|let|for|in|if|while|return|await|spawn)\b|\b(?:string|float|int|bool)\b|\b[A-Z]\w*\b)/;

function tone(part: string): string | undefined {
  if (part.startsWith('//')) return 'text-white/35';
  if (part.startsWith('"') || part.startsWith('`'))
    return 'text-(--color-pasture-200)';
  if (/^[a-z]+$/.test(part)) {
    return /^(string|float|int|bool)$/.test(part)
      ? 'text-white'
      : 'text-clover';
  }
  return /^[A-Z]/.test(part) ? 'text-white' : undefined;
}

export function Highlight({ line }: { line: string }): ReactNode {
  return line.split(TOKENS).map((part, i) => {
    const key = `${i}-${part}`;
    return i % 2 ? (
      <span className={tone(part)} key={key}>
        {part}
      </span>
    ) : (
      part
    );
  });
}

export type LineMark = 'added' | 'removed' | 'running';

const MARK: Record<LineMark, string> = {
  added: 'bg-(--color-pasture-200)/12',
  removed: 'bg-[#E5484D]/14',
  running: 'bg-clover/12',
};

/** Numbered source lines, with an optional mark on any of them. Lines listed in prose are a prompt's text. */
export function Code({
  lines,
  marks = {},
  prose = [],
}: {
  lines: string[];
  marks?: Record<number, LineMark>;
  prose?: number[];
}) {
  return (
    <div className="font-mono text-[12.5px] md:text-[14px] leading-[22px] md:leading-6 text-[#BDBDC4]">
      {lines.map((line, i) => (
        <div
          className={`flex pr-4 transition-colors duration-200 ${marks[i] ? MARK[marks[i]] : ''}`}
          data-line={i}
          // biome-ignore lint/suspicious/noArrayIndexKey: a sample's lines keep their positions
          key={i}
        >
          <span className="shrink-0 w-10 md:w-12 pr-4 text-right text-white/20 select-none">
            {i + 1}
          </span>
          <span className="whitespace-pre min-w-0">
            {prose.includes(i) ? (
              <span className="text-(--color-pasture-200)">{line}</span>
            ) : (
              <Highlight line={line} />
            )}
          </span>
        </div>
      ))}
    </div>
  );
}

/** The text inside a multi-line prompt: prose in the string color, with each ${...} in white. */
export function Prose({ line }: { line: string }): ReactNode {
  return line.split(/(\$\{\w+\})/).map((part, i) => (
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
