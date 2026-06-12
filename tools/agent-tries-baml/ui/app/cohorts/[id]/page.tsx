import Link from 'next/link';
import { notFound } from 'next/navigation';

import { BackLink, PageHeader } from '@/components/page-header';
import { DataTable, Td, Th, Tr } from '@/components/ui/data-table';
import { InlineCode } from '@/components/ui/inline-code';
import { StatPill, type StatPillTone } from '@/components/ui/stat-pill';

import { loadCohort, type CohortVariant } from '../../lib/data';
import ReportMd from '../../runs/[id]/report-md';

export const dynamic = 'force-dynamic';

const outcomeTone = (outcome: string): StatPillTone =>
  outcome === 'success' ? 'success' : outcome === 'failed' ? 'destructive' : 'mute';

/**
 * A number with a hairline meter underneath showing it relative to the cohort
 * max, so turns/cost compare at a glance without reading every digit.
 */
function Meter({ value, max, fmt }: { value: number | null; max: number; fmt: (v: number) => string }) {
  if (value == null) return <span className="text-muted-foreground">-</span>;
  const pct = max > 0 ? Math.max(4, Math.round((value / max) * 100)) : 0;
  return (
    <span className="inline-block min-w-[64px]">
      <span className="mono block">{fmt(value)}</span>
      <span className="mt-[3px] block h-[3px] w-full bg-muted">
        <span
          className="block h-full bg-border"
          style={{ width: `${pct}%` }}
          aria-hidden
        />
      </span>
    </span>
  );
}

/** The cheapest successful variant — the row worth reading first. */
function bestOf(variants: CohortVariant[]): CohortVariant | null {
  const done = variants.filter((v) => v.outcome === 'success' && v.costUsd != null);
  if (done.length < 2) return null; // a "winner" needs something to beat
  return done.reduce((a, b) => ((a.costUsd ?? 0) <= (b.costUsd ?? 0) ? a : b));
}

/**
 * Server component for the "/cohorts/[id]" route: a skill-arena cohort's variant
 * runs (one per baml-skill branch) with relative turn/cost meters and the lowest-cost
 * success highlighted, followed by the synthesized comparison report inline so the
 * whole story reads on one page. 404s if not found.
 * @param params - the route params resolving to the cohort id
 * @returns the cohort detail page, or a not-found response
 */
export default async function CohortPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const d = await loadCohort(id);
  if (!d) notFound();
  const { cohort, variants, reportTrophyId, report } = d;
  const maxTurns = Math.max(0, ...variants.map((v) => v.turns ?? 0));
  const maxCost = Math.max(0, ...variants.map((v) => v.costUsd ?? 0));
  const best = bestOf(variants);
  return (
    <div>
      <PageHeader
        back={<BackLink href="/cohorts">← skill arenas</BackLink>}
        title={<InlineCode text={cohort.prompt} />}
      >
        <p>
          <strong>{cohort.status}</strong> · {variants.length} variants ·
          branches:{' '}
          <span className="mono">{(cohort.skillRefs ?? []).join(', ')}</span>
          {reportTrophyId ? (
            <>
              {' '}
              · <Link href={`/runs/${reportTrophyId}`}>full report run →</Link>
            </>
          ) : null}
        </p>
      </PageHeader>

      <h2 className="mb-3 text-2xl font-bold">Variants ({variants.length})</h2>
      {variants.length === 0 ? (
        <p className="text-muted-foreground">no variants yet.</p>
      ) : (
        <DataTable>
          <thead>
            <tr>
              <Th>branch</Th>
              <Th>outcome</Th>
              <Th align="right">turns</Th>
              <Th align="right">cost</Th>
              <Th align="right">findings</Th>
              <Th>run</Th>
            </tr>
          </thead>
          <tbody>
            {variants.map((v) => {
              const isBest = best != null && v.taskId === best.taskId;
              return (
                <Tr key={v.taskId} className={isBest ? 'bg-muted' : undefined}>
                  <Td className="mono">
                    {v.skillRef ?? '—'}
                    {isBest ? (
                      <span
                        className="ml-1.5 font-mono text-[11px] uppercase tracking-[0.04em] text-success"
                        title="lowest-cost success in this cohort"
                      >
                        best
                      </span>
                    ) : null}
                  </Td>
                  <Td>
                    {v.outcome ? (
                      <StatPill tone={outcomeTone(v.outcome)}>{v.outcome}</StatPill>
                    ) : (
                      <StatPill tone="mute">{v.status}</StatPill>
                    )}
                  </Td>
                  <Td align="right">
                    <Meter value={v.turns} max={maxTurns} fmt={(n) => String(n)} />
                  </Td>
                  <Td align="right">
                    <Meter
                      value={v.costUsd}
                      max={maxCost}
                      fmt={(n) => `$${n.toFixed(2)}`}
                    />
                  </Td>
                  <Td align="right" className="mono">{v.findings || ''}</Td>
                  <Td>
                    {v.trophyId ? (
                      <Link href={`/runs/${v.trophyId}`}>run →</Link>
                    ) : (
                      <span className="text-muted-foreground">—</span>
                    )}
                  </Td>
                </Tr>
              );
            })}
          </tbody>
        </DataTable>
      )}

      {variants.some((v) => v.skillText) ? (
        <section className="mt-7">
          <h2 className="mb-3 text-2xl font-bold">Skills</h2>
          <p className="mb-3 text-[13px] text-muted-foreground">
            The exact skill text each variant onboarded from (snapshotted by the
            worker at run time).
          </p>
          {variants
            .filter((v) => v.skillText)
            .map((v) => (
              <details key={v.taskId} className="mb-2 border border-border">
                <summary className="cursor-pointer px-3 py-2 font-mono text-[12.5px]">
                  {v.skillRef ?? 'static skill'}{' '}
                  <span className="text-muted-foreground">
                    · {(v.skillText as string).split('\n').length} lines ·{' '}
                    {Math.round((v.skillText as string).length / 1024)} KB
                  </span>
                </summary>
                <pre className="max-h-[480px] overflow-auto border-t border-border bg-muted px-3 py-2 font-mono text-[11.5px] leading-[1.45] whitespace-pre-wrap">
                  {v.skillText}
                </pre>
              </details>
            ))}
        </section>
      ) : null}

      {report?.summary ? (
        <section className="md-body mt-7">
          <p>{report.summary}</p>
        </section>
      ) : null}

      {report?.reportMd ? (
        <section className="mt-7">
          <h2 className="mb-3 text-2xl font-bold">Comparison report</h2>
          <ReportMd>{report.reportMd}</ReportMd>
        </section>
      ) : cohort.status !== 'done' && cohort.status !== 'failed' ? (
        <p className="mt-7 text-muted-foreground">
          The comparison report appears here once every variant finishes and
          cohort-compare runs.
        </p>
      ) : null}
    </div>
  );
}
