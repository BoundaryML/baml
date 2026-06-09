import Link from 'next/link';

import { BackLink, PageHeader } from '@/components/page-header';
import { DataTable, Td, Th, Tr } from '@/components/ui/data-table';
import { InlineCode } from '@/components/ui/inline-code';
import { StatPill, type StatPillTone } from '@/components/ui/stat-pill';

import { loadState } from '../lib/data';
import { ago } from '../lib/format';

export const dynamic = 'force-dynamic';

const statusTone = (status: string): StatPillTone =>
  status === 'done' ? 'success' : status === 'failed' ? 'destructive' : 'mute';

/**
 * Server component for the "/cohorts" route: a table of skill-arena cohorts —
 * status, task, branches, age — each row linking to its cohort detail. A table
 * (not cards) so a season of arenas scans top-to-bottom in one column.
 * @returns the cohorts landing page
 */
export default async function CohortsPage() {
  const s = await loadState();
  const cohorts = s.cohorts ?? [];
  const now = Date.now();
  return (
    <div>
      <PageHeader
        back={<BackLink href="/">← dashboard</BackLink>}
        title="skill arenas"
      >
        <p>
          {cohorts.length} cohort(s) · each runs one task across several
          baml-skill branches, then compares the outcomes.
        </p>
      </PageHeader>
      {cohorts.length === 0 ? (
        <p className="text-muted-foreground">
          no cohorts yet. trigger one with{' '}
          <span className="mono">@bot [skill arena] &lt;task&gt;</span>.
        </p>
      ) : (
        <DataTable>
          <thead>
            <tr>
              <Th>status</Th>
              <Th>task</Th>
              <Th>branches</Th>
              <Th align="right">variants</Th>
              <Th align="right">age</Th>
            </tr>
          </thead>
          <tbody>
            {cohorts.map((c) => (
              <Tr key={c._id}>
                <Td>
                  <StatPill tone={statusTone(c.status)}>{c.status}</StatPill>
                </Td>
                <Td cell="task">
                  <Link href={`/cohorts/${c._id}`} title={c.prompt}>
                    <InlineCode text={c.prompt.slice(0, 120)} />
                  </Link>
                </Td>
                <Td className="mono text-muted-foreground">
                  {(c.skillRefs ?? []).join(', ')}
                </Td>
                <Td align="right" className="mono">
                  {(c.skillRefs ?? []).length}
                </Td>
                <Td align="right" className="mono text-muted-foreground">
                  {ago(now - c.createdAt)}
                </Td>
              </Tr>
            ))}
          </tbody>
        </DataTable>
      )}
    </div>
  );
}
