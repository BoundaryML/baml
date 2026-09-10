import Link from 'next/link';
import { activityText, eventProposalPath } from '@/lib/activity';
import type { IssueEvent } from '@/lib/db';

export function Activity({ events }: { events: IssueEvent[] }) {
  return (
    <section className="space-y-4">
      <h2 className="text-lg font-semibold">Activity</h2>
      {events.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No activity recorded yet.
        </p>
      ) : (
        <ol className="space-y-4 border-l pl-5">
          {events.map((event) => {
            const proposal = eventProposalPath(event);
            return (
              <li className="space-y-1" key={event.id}>
                <time
                  className="text-xs text-muted-foreground"
                  dateTime={event.created_at}
                >
                  {new Date(event.created_at).toLocaleString('en-US', {
                    timeZone: 'UTC',
                  })}{' '}
                  UTC
                </time>
                <p>{activityText(event)}</p>
                {proposal && (
                  <Link className="text-sm underline" href={proposal}>
                    View proposed fix and approval
                  </Link>
                )}
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
