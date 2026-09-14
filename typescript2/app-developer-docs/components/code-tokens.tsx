import type { ThemedToken } from 'shiki';
import {
  type CodeHighlight,
  highlightLabels,
  type resolveCodeHighlights,
} from '@/lib/snippets/code-highlights';

export function CodeTokens({
  className,
  lines,
  ranges,
}: {
  className?: string;
  lines: ThemedToken[][];
  ranges: ReturnType<typeof resolveCodeHighlights>;
}) {
  let offset = 0;
  return (
    <code className={className}>
      {lines.map((line, index) => {
        const lineOffset = offset;
        const tokens = line.flatMap((token) => {
          const start = offset;
          const end = start + token.content.length;
          offset = end;
          const boundaries = [
            ...new Set([
              start,
              end,
              ...ranges.flatMap((range) =>
                [range.start, range.end].filter(
                  (value) => value > start && value < end,
                ),
              ),
            ]),
          ].sort((a, b) => a - b);
          return boundaries.slice(0, -1).map((position, part) => {
            const range = ranges.find(
              (item) => position >= item.start && position < item.end,
            );
            return (
              <span
                className={range ? 'code-highlight' : undefined}
                data-highlight={range?.kind}
                key={position}
                style={{ color: token.color }}
                title={range ? highlightLabels[range.kind] : undefined}
              >
                {token.content.slice(
                  position - start,
                  boundaries[part + 1] - start,
                )}
              </span>
            );
          });
        });
        offset += 1;
        return (
          <span key={lineOffset}>
            {tokens}
            {index < lines.length - 1 ? '\n' : null}
          </span>
        );
      })}
    </code>
  );
}

export function CodeHighlightKey({
  highlights,
}: {
  highlights: CodeHighlight[];
}) {
  const kinds: CodeHighlight['kind'][] = ['success', 'syntax', 'recovery'];
  return (
    <div className="code-highlight-key not-typeset">
      {kinds
        .filter((kind) => highlights.some((item) => item.kind === kind))
        .map((kind) => (
          <span data-highlight={kind} key={kind}>
            {highlightLabels[kind]}
          </span>
        ))}
    </div>
  );
}
