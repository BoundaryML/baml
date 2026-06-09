import Link from 'next/link';
import { notFound } from 'next/navigation';

import { BackLink, PageHeader } from '@/components/page-header';
import { InlineCode } from '@/components/ui/inline-code';
import { StatPill } from '@/components/ui/stat-pill';

import { loadRun, type Turn } from '../../lib/data';
import CallScroller from './call-scroller';
import ExpandAll from './expand-all';
import FilesIde from './files-ide';
import ReportMd from './report-md';
import TranscriptTabs from './transcript-tabs';

export const dynamic = 'force-dynamic';

type Params = { id: string };

const int = (n: unknown) =>
  typeof n === 'number' ? Math.round(n).toLocaleString() : '-';

/**
 * A single labeled metric cell in the run's metrics grid.
 * @param k - the metric label
 * @param v - the metric value to render
 */
function Stat({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="bg-background px-3 py-2.5">
      <div className="text-[11px] uppercase tracking-[0.05em] text-muted-foreground">
        {k}
      </div>
      <div className="mono mt-[3px] text-[19px]">{v}</div>
    </div>
  );
}

/**
 * Server component for the "/runs/[id]" route rendering a trophy's full detail:
 * header, summary, metrics grid, what-went-well/failed, findings, and the agent
 * transcript. 404s if the trophy is not found. Includes the client CallScroller
 * (deep-links via ?call=N) and ExpandAll control.
 * @param params - the route params resolving to the trophy id
 * @returns the run detail page, or a not-found response
 */
export default async function RunPage({ params }: { params: Promise<Params> }) {
  const { id } = await params;
  const detail = await loadRun(id);
  if (!detail) notFound();
  const { trophy, task, bamlLabel, transcriptText } = detail;
  const m = trophy.metrics ?? {};
  const turns = trophy.turnLog ?? [];
  const toolCalls =
    m.tool_calls ?? turns.reduce((a, t) => a + (t.tools?.length ?? 0), 0);
  const wall =
    typeof m.wall_clock_ms === 'number'
      ? `${Math.round(m.wall_clock_ms / 1000)}s`
      : '-';
  const cost =
    typeof m.estimated_cost_usd === 'number'
      ? `$${m.estimated_cost_usd.toFixed(2)}`
      : '-';
  const filesCreated = Object.entries(trophy.filesCreated ?? {});

  return (
    <div>
      <CallScroller />
      <PageHeader
        back={
          <>
            <BackLink href="/">← dashboard</BackLink>
            {'   ·   '}
            <BackLink href="/db/tasks">task</BackLink>
          </>
        }
        title={<InlineCode text={task?.prompt ?? '(task not found)'} />}
      >
        <p>
          <strong>{trophy.outcome}</strong>
          {trophy.isCohortReport ? (
            <>
              {' '}
              · <StatPill tone="link">cohort report</StatPill>
              {trophy.cohortId ? (
                <>
                  {' '}
                  · <Link href={`/cohorts/${trophy.cohortId}`}>view cohort</Link>
                </>
              ) : null}
            </>
          ) : null}
          {task ? <> · {task.source}</> : null}
          {task?.skillRef ? (
            <>
              {' '}
              · skill <span className="mono">{task.skillRef}</span>
            </>
          ) : null}
          {task?.cohortId && !trophy.isCohortReport ? (
            <>
              {' '}
              · <Link href={`/cohorts/${task.cohortId}`}>arena →</Link>
            </>
          ) : null}
          {bamlLabel ? (
            <>
              {' '}
              · baml <span className="mono">{bamlLabel}</span>
            </>
          ) : trophy.bamlVersion ? (
            <>
              {' '}
              · baml{' '}
              <span className="mono">
                {trophy.bamlVersion === 'coldstart'
                  ? 'cold start'
                  : trophy.bamlVersion.slice(0, 8)}
              </span>
            </>
          ) : null}
          {' · '}
          <span className="mono text-muted-foreground">
            {trophy._id.slice(0, 10)}
          </span>
        </p>
      </PageHeader>

      {trophy.summary ? (
        <section className="md-body" style={{ margin: '16px 0' }}>
          <p>{trophy.summary}</p>
        </section>
      ) : null}

      <section className="my-4 mb-[26px]">
        {/* gap-px + bg-border fakes 1px borders between cells */}
        <div className="grid grid-cols-[repeat(auto-fit,minmax(94px,1fr))] gap-px border border-border bg-border">
          <Stat k="turns" v={int(m.turns)} />
          <Stat k="tool calls" v={int(toolCalls)} />
          <Stat k="api calls" v={int(m.api_calls)} />
          <Stat k="in tokens" v={int(m.input_tokens)} />
          <Stat k="out tokens" v={int(m.output_tokens)} />
          <Stat k="files" v={int(m.files_touched ?? filesCreated.length)} />
          <Stat k="loc" v={int(m.loc_changed)} />
          <Stat k="wall" v={wall} />
          <Stat k="cost" v={cost} />
        </div>
      </section>

      {/* jump bar: long run pages get one-click section access */}
      <nav
        aria-label="run sections"
        className="sticky top-0 z-10 -mx-1 mb-2 flex items-baseline gap-4 overflow-x-auto border-b border-border bg-background px-1 py-2 font-mono text-[12px]"
      >
        <span className="text-muted-foreground">jump:</span>
        {trophy.reportMd ? (
          <a href="#report" className="whitespace-nowrap no-underline hover:text-link">
            report
          </a>
        ) : null}
        {trophy.findings?.length ? (
          <a href="#findings" className="whitespace-nowrap no-underline hover:text-link">
            findings ({trophy.findings.length})
          </a>
        ) : null}
        {filesCreated.length > 0 ? (
          <a href="#files" className="whitespace-nowrap no-underline hover:text-link">
            files ({filesCreated.length})
          </a>
        ) : null}
        {turns.length > 0 || transcriptText ? (
          <a href="#transcript" className="whitespace-nowrap no-underline hover:text-link">
            transcript ({turns.length})
          </a>
        ) : null}
      </nav>

      {trophy.whatWentWell?.length || trophy.whatFailed?.length ? (
        <section className="my-2 mb-6 grid grid-cols-2 gap-x-7 gap-y-1 max-[640px]:grid-cols-1">
          {trophy.whatWentWell?.length ? (
            <div>
              <h3 className="text-[1.17em] font-bold">What went well</h3>
              <ul className="mt-1.5 list-disc pl-[18px] [&>li]:my-1">
                {trophy.whatWentWell.map((x, i) => (
                  <li key={i}>{x}</li>
                ))}
              </ul>
            </div>
          ) : null}
          {trophy.whatFailed?.length ? (
            <div>
              <h3 className="text-[1.17em] font-bold">What failed</h3>
              <ul className="mt-1.5 list-disc pl-[18px] [&>li]:my-1">
                {trophy.whatFailed.map((x, i) => (
                  <li key={i}>{x}</li>
                ))}
              </ul>
            </div>
          ) : null}
        </section>
      ) : null}

      {trophy.reportMd ? (
        <section id="report" className="report-md mt-5 scroll-mt-12">
          <details open className="run-block">
            <summary>Report</summary>
            <ReportMd>{trophy.reportMd}</ReportMd>
          </details>
        </section>
      ) : null}

      {trophy.findings && trophy.findings.length > 0 ? (
        <section id="findings" className="findings mt-5 scroll-mt-12">
          <h2 className="text-2xl font-bold">
            Findings ({trophy.findings.length})
          </h2>
          {trophy.findings.map((f, i) => {
            const call = f.anchor?.call_index;
            return (
              <details key={i} className="run-block">
                <summary>
                  <StatPill tone={f.kind === 'language' ? 'destructive' : 'mute'}>
                    {f.kind}
                  </StatPill>{' '}
                  {f.reproVerified ? (
                    <StatPill tone="success">repro verified</StatPill>
                  ) : null}{' '}
                  <InlineCode text={f.title} />
                  {call != null ? (
                    <a href={`#call-${call}`} className="mono text-muted-foreground">
                      {' '}
                      · call {call}
                    </a>
                  ) : null}
                </summary>
                <p className="mt-2">{f.description}</p>
                {f.repro ? <pre className="tool-input">{f.repro}</pre> : null}
              </details>
            );
          })}
        </section>
      ) : null}

      {filesCreated.length > 0 ? (
        <section id="files" className="mt-5 scroll-mt-12">
          <h2 className="mb-3 text-2xl font-bold">
            Files created ({filesCreated.length})
          </h2>
          <FilesIde files={filesCreated} />
        </section>
      ) : null}

      {turns.length > 0 || transcriptText ? (
        <section id="transcript" className="transcript scroll-mt-12">
          <h2 className="flex items-baseline gap-3 text-2xl font-bold">
            agent transcript ({turns.length} calls)
            <ExpandAll />
          </h2>
          <p className="mb-3 text-sm text-muted-foreground">
            Each call has an anchor <span className="mono">#call-N</span>; open
            with <span className="mono">?call=N</span> to jump to it (evidence
            links). Switch to <span className="mono">raw</span> for the full
            Ctrl-F-able transcript.
          </p>
          <TranscriptTabs
            raw={transcriptText}
            structured={turns.map((t: Turn) => (
              <TurnBlock key={t.i} turn={t} />
            ))}
          />
        </section>
      ) : null}
    </div>
  );
}

/**
 * A collapsible transcript block for one agent call, anchored at #call-N. Shows the
 * turn's thinking and assistant-text previews plus each tool call's input and result.
 * @param turn - the transcript turn to render
 */
function TurnBlock({ turn }: { turn: Turn }) {
  return (
    <details
      className="run-block call-block"
      id={`call-${turn.i}`}
      data-call-index={turn.i}
    >
      <summary>
        <span className="mono">#{turn.i}</span>
        {turn.ts ? (
          <span className="mono text-muted-foreground">
            {' '}
            · {turn.ts.slice(11, 19)}
          </span>
        ) : null}
        {turn.text_chars ? (
          <span className="text-muted-foreground">
            {' '}
            · {turn.text_chars} text chars
          </span>
        ) : null}
        {turn.tools && turn.tools.length > 0 ? (
          <span className="text-muted-foreground">
            {' · '}
            {turn.tools.map((t) => t.name ?? 'tool').join(', ')}
          </span>
        ) : null}
      </summary>
      {turn.thinking_preview ? (
        <details className="run-subblock">
          <summary>thinking ({turn.thinking_chars ?? 0} chars)</summary>
          <pre>{turn.thinking_preview}</pre>
        </details>
      ) : null}
      {turn.text_preview ? (
        <details className="run-subblock" open>
          <summary>assistant text ({turn.text_chars ?? 0} chars)</summary>
          <pre>{turn.text_preview}</pre>
        </details>
      ) : null}
      {turn.tools?.map((tool, idx) => (
        <details key={idx} className="run-subblock">
          <summary>
            tool: <span className="mono">{tool.name ?? '?'}</span>
            {tool.is_error ? (
              <span className="text-destructive"> (error)</span>
            ) : null}
            {tool.result_chars ? (
              <span className="text-muted-foreground">
                {' '}
                · {tool.result_chars} chars
              </span>
            ) : null}
          </summary>
          {tool.input ? (
            <pre className="tool-input">
              {JSON.stringify(tool.input, null, 2)}
            </pre>
          ) : null}
          {tool.result_preview ? (
            <pre className="tool-result">{tool.result_preview}</pre>
          ) : null}
        </details>
      ))}
    </details>
  );
}
