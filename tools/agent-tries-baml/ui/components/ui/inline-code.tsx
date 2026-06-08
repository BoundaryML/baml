import { cn } from '@/lib/utils';

/**
 * Renders a plain-text title/prompt with markdown-style `inline code` spans
 * as real <code> elements (issue titles and task prompts are markdown-ish).
 * Styling matches the md-body inline-code look.
 * @param text - the raw text possibly containing `backtick` spans
 */
export function InlineCode({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const parts = text.split(/(`[^`]+`)/g);
  if (parts.length === 1) return <>{text}</>;
  return (
    <>
      {parts.map((p, i) =>
        p.length > 2 && p.startsWith('`') && p.endsWith('`') ? (
          <code
            key={i}
            className={cn(
              'rounded bg-[rgba(109,40,217,0.06)] px-1 font-mono text-[0.82em] text-[#4a3a6b] [overflow-wrap:anywhere]',
              className,
            )}
          >
            {p.slice(1, -1)}
          </code>
        ) : (
          p
        ),
      )}
    </>
  );
}
