import Link from 'next/link';
import { FormattedText } from '@/components/code';
import { SubsystemBadge } from '@/components/issues/issue-status';
import { Badge } from '@/components/ui/badge';
import type { Intuition } from '@/lib/types';

const KIND_LABEL: Record<Intuition['kind'], string> = {
  Hotspot: 'hotspot',
  Pattern: 'pattern',
  Process: 'process',
  SharedCause: 'shared cause',
};

const CONFIDENCE_CLASS: Record<Intuition['confidence'], string> = {
  high: 'border-transparent bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-200',
  low: 'border-transparent bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  medium:
    'border-transparent bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200',
};

/** One cross-issue finding. `current` is the issue page it sits on, so its own link is dropped. */
export function IntuitionCard({
  intuition,
  current,
}: {
  intuition: Intuition;
  current?: string;
}) {
  const others = intuition.issue_ids.filter((id) => id !== current);
  return (
    <article className="min-w-0 rounded-lg border bg-card p-4 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Badge className="font-normal" variant="outline">
          {KIND_LABEL[intuition.kind]}
        </Badge>
        <SubsystemBadge subsystem={intuition.subsystem} />
        <Badge
          className={CONFIDENCE_CLASS[intuition.confidence]}
          variant="outline"
        >
          {intuition.confidence} confidence
        </Badge>
        <span className="ml-auto text-xs text-muted-foreground">
          {new Date(intuition.generated_at).toLocaleDateString('en-US', {
            day: 'numeric',
            month: 'short',
            timeZone: 'UTC',
          })}
        </span>
      </div>
      <h3 className="text-base font-semibold leading-snug break-words">
        {intuition.title}
      </h3>
      <FormattedText text={intuition.insight} />
      <details className="text-sm">
        <summary className="cursor-pointer text-muted-foreground">
          Evidence
        </summary>
        <div className="mt-2">
          <FormattedText text={intuition.evidence} />
        </div>
      </details>
      <div className="rounded-md bg-muted/50 p-3 text-sm">
        <span className="font-medium">Suggested next step: </span>
        {intuition.suggested_action}
      </div>
      {others.length > 0 && (
        <div className="flex flex-wrap gap-2 text-xs">
          <span className="text-muted-foreground">
            {current ? 'Also cites' : 'Cites'}
          </span>
          {others.map((id) => (
            <Link
              className="font-mono underline"
              href={`/issues/${id}`}
              key={id}
            >
              {id}
            </Link>
          ))}
        </div>
      )}
    </article>
  );
}
