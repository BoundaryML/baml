import type { TranscriptEvent } from './agent-terminal';
import { FormattedText } from './code';

export function AgentResult({ events }: { events: TranscriptEvent[] }) {
  let result: Record<string, unknown> | null = null;
  for (const event of [...events].reverse()) {
    if (
      event.name !== 'StructuredOutput' &&
      !(event.role === 'assistant' && event.type === 'text')
    )
      continue;
    try {
      const parsed = JSON.parse(event.text);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        result = parsed;
        break;
      }
    } catch {
      /* Ordinary transcript text. */
    }
  }
  if (!result) return null;
  const difficulty =
    typeof result.difficulty === 'string' &&
    /^(Trivial|Easy|Medium|Hard)$/.test(result.difficulty)
      ? result.difficulty
      : null;
  const reason =
    typeof result.rationale === 'string'
      ? result.rationale
      : typeof result.reason === 'string'
        ? result.reason
        : null;
  const questions = Array.isArray(result.unknowns)
    ? result.unknowns.filter((s): s is string => typeof s === 'string')
    : [];
  if (!difficulty || !reason) return null;
  return (
    <section className="rounded-xl border bg-card p-5">
      <div className="flex items-center gap-3">
        <h2 className="text-sm font-medium">Difficulty assessment</h2>
        <span className="rounded-full bg-blue-500/10 px-2.5 py-1 text-xs font-medium text-blue-700 dark:text-blue-300">
          {difficulty}
        </span>
      </div>
      <FormattedText text={reason} />
      {questions.length > 0 && (
        <details className="mt-3 border-t pt-3">
          <summary className="cursor-pointer text-xs text-muted-foreground">
            Open questions · {questions.length}
          </summary>
          <ul className="mt-3 list-disc space-y-2 pl-4 text-sm text-muted-foreground">
            {questions.map((q) => (
              <li key={q}>{q}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
