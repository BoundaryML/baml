import { notFound } from 'next/navigation';

import { BackLink, PageHeader } from '@/components/page-header';
import { InlineCode } from '@/components/ui/inline-code';
import {
  CardGrid,
  IssueCard,
  IssueCardMeta,
  IssueCardTitle,
  KindTag,
} from '@/components/ui/issue-board';
import { StatPill } from '@/components/ui/stat-pill';

import { issueStatusLabel, loadIssue } from '../../lib/data';
import { ago } from '../../lib/format';
import { getNotionContent } from '../../lib/notion';
import ReportMd from '../../runs/[id]/report-md';

export const dynamic = 'force-dynamic';

/**
 * Server component for the "/issues/[id]" route: the full issue write-up
 * (markdown description) plus its evidence runs, each linking to the
 * anchored transcript call that surfaced it.
 * @param params - the route params resolving to the issue id
 * @returns the issue detail page, or a not-found response
 */
export default async function IssuePage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const d = await loadIssue(id);
  if (!d) notFound();
  const { issue, evidenceRuns } = d;
  const label = issueStatusLabel(issue);
  const now = Date.now();
  const notion = issue.notionPageId
    ? await getNotionContent(issue.notionPageId)
    : null;
  return (
    <div>
      <PageHeader
        back={<BackLink href="/#issues">← issues</BackLink>}
        title={<InlineCode text={issue.title} />}
      >
        <p>
          <StatPill
            tone={
              label === 'failed'
                ? 'destructive'
                : label === 'cursor'
                  ? 'link'
                  : 'success'
            }
          >
            {label}
          </StatPill>{' '}
          · <KindTag kind={issue.kind}>{issue.kind}</KindTag> · first seen{' '}
          {ago(now - issue.firstSeenAt)} ago · last seen{' '}
          {ago(now - issue.lastSeenAt)} ago
          {notion ? (
            <>
              {' '}
              ·{' '}
              <a href={notion.url} target="_blank" rel="noreferrer">
                notion ↗
              </a>
            </>
          ) : null}
        </p>
      </PageHeader>

      {/* prefer the live Notion page content (it may carry human edits);
          fall back to the synced description from convex */}
      <ReportMd>{notion?.contentMd ?? issue.description}</ReportMd>

      {notion && notion.comments.length > 0 ? (
        <>
          <h2 className="text-2xl font-bold">
            Comments ({notion.comments.length})
          </h2>
          <div className="mb-[18px] flex flex-col gap-2.5">
            {notion.comments.map((c, i) => (
              <div key={i} className="border-l-[3px] border-border py-1 pl-3">
                <div className="mb-0.5 text-[12.5px]">
                  <b>{c.author}</b>{' '}
                  <span className="mono text-muted-foreground">
                    {ago(now - new Date(c.createdAt).getTime())} ago
                  </span>
                </div>
                <ReportMd>{c.text}</ReportMd>
              </div>
            ))}
          </div>
        </>
      ) : null}

      <h2 className="text-2xl font-bold">Evidence ({evidenceRuns.length})</h2>
      {evidenceRuns.length === 0 ? (
        <p className="text-muted-foreground">no linked runs.</p>
      ) : (
        <CardGrid>
          {evidenceRuns.map((e, i) => (
            <IssueCard
              key={`${e.trophyId}-${i}`}
              href={
                e.callIndex != null
                  ? `/runs/${e.trophyId}#call-${e.callIndex}`
                  : `/runs/${e.trophyId}`
              }
              title={e.prompt ?? undefined}
            >
              <KindTag kind={e.outcome ?? 'partial'}>{e.outcome ?? 'run'}</KindTag>
              <IssueCardTitle>
                <InlineCode text={(e.prompt ?? e.trophyId.slice(0, 12)).slice(0, 120)} />
              </IssueCardTitle>
              <IssueCardMeta>
                {e.turns ?? '–'} turns · ${(e.costUsd ?? 0).toFixed(2)} ·{' '}
                {e.callIndex != null ? `call ${e.callIndex}` : 'run'}
                {e.createdAt ? ` · ${ago(now - e.createdAt)}` : ''}
              </IssueCardMeta>
            </IssueCard>
          ))}
        </CardGrid>
      )}
    </div>
  );
}
