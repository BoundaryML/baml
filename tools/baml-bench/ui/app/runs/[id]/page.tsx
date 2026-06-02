import Link from "next/link";
import { notFound } from "next/navigation";

import { loadRun, type Turn } from "../../lib/data";
import CallScroller from "./call-scroller";
import ExpandAll from "./expand-all";

export const dynamic = "force-dynamic";

type Params = { id: string };

const int = (n: unknown) => (typeof n === "number" ? Math.round(n).toLocaleString() : "-");

/**
 * A single labeled metric cell in the run's metrics grid.
 * @param k - the metric label
 * @param v - the metric value to render
 */
function Stat({ k, v }: { k: string; v: React.ReactNode }) {
  return <div className="mstat"><div className="mk">{k}</div><div className="mv mono">{v}</div></div>;
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
  const { trophy, task, bamlLabel } = detail;
  const m = trophy.metrics ?? {};
  const turns = trophy.turnLog ?? [];
  const toolCalls = m.tool_calls ?? turns.reduce((a, t) => a + (t.tools?.length ?? 0), 0);
  const wall = typeof m.wall_clock_ms === "number" ? `${Math.round(m.wall_clock_ms / 1000)}s` : "-";
  const cost = typeof m.estimated_cost_usd === "number" ? `$${m.estimated_cost_usd.toFixed(2)}` : "-";

  return (
    <div>
      <CallScroller />
      <header className="page">
        <p style={{ marginBottom: 6 }}>
          <Link href="/" className="back-link">← dashboard</Link>
          {"   ·   "}
          <Link href="/db/tasks" className="back-link">task</Link>
        </p>
        <h1>{task?.prompt ?? "(task not found)"}</h1>
        <p>
          <strong>{trophy.outcome}</strong>
          {task ? <> · {task.source}</> : null}
          {bamlLabel ? (
            <> · baml <span className="mono">{bamlLabel}</span></>
          ) : trophy.bamlVersion ? (
            <> · baml <span className="mono">{trophy.bamlVersion.slice(0, 8)}</span></>
          ) : null}
          {" · "}<span className="mono mute">{trophy._id.slice(0, 10)}</span>
        </p>
      </header>

      {trophy.summary ? (
        <section className="md-body" style={{ margin: "16px 0" }}><p>{trophy.summary}</p></section>
      ) : null}

      <section className="metrics">
        <div className="mgrid">
          <Stat k="turns" v={int(m.turns)} />
          <Stat k="tool calls" v={int(toolCalls)} />
          <Stat k="api calls" v={int(m.api_calls)} />
          <Stat k="in tokens" v={int(m.input_tokens)} />
          <Stat k="out tokens" v={int(m.output_tokens)} />
          <Stat k="wall" v={wall} />
          <Stat k="cost" v={cost} />
        </div>
      </section>

      {(trophy.whatWentWell?.length || trophy.whatFailed?.length) ? (
        <section className="cols">
          {trophy.whatWentWell?.length ? (
            <div><h3>What went well</h3><ul>{trophy.whatWentWell.map((x, i) => <li key={i}>{x}</li>)}</ul></div>
          ) : null}
          {trophy.whatFailed?.length ? (
            <div><h3>What failed</h3><ul>{trophy.whatFailed.map((x, i) => <li key={i}>{x}</li>)}</ul></div>
          ) : null}
        </section>
      ) : null}

      {trophy.findings && trophy.findings.length > 0 ? (
        <section style={{ marginTop: 20 }}>
          <h2>Findings ({trophy.findings.length})</h2>
          {trophy.findings.map((f, i) => {
            const call = f.anchor?.call_index;
            return (
              <details key={i} className="run-block">
                <summary>
                  <span className={`statpill ${f.kind === "language" ? "failed" : "partial"}`}>{f.kind}</span>{" "}
                  {f.reproVerified ? <span className="statpill completed">repro verified</span> : null}{" "}
                  {f.title}
                  {call != null ? <a href={`#call-${call}`} className="mono mute"> · call {call}</a> : null}
                </summary>
                <p style={{ marginTop: 8 }}>{f.description}</p>
                {f.repro ? <pre className="tool-input">{f.repro}</pre> : null}
              </details>
            );
          })}
        </section>
      ) : null}

      {turns.length > 0 ? (
        <section id="transcript" className="transcript">
          <h2 style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
            agent transcript ({turns.length} calls)
            <ExpandAll />
          </h2>
          <p className="mute" style={{ marginBottom: 12, fontSize: 14 }}>
            Each call has an anchor <span className="mono">#call-N</span>; open with{" "}
            <span className="mono">?call=N</span> to jump to it (evidence links).
          </p>
          {turns.map((t: Turn) => (
            <TurnBlock key={t.i} turn={t} />
          ))}
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
    <details className="run-block call-block" id={`call-${turn.i}`} data-call-index={turn.i}>
      <summary>
        <span className="mono">#{turn.i}</span>
        {turn.text_chars ? <span className="mute"> · {turn.text_chars} text chars</span> : null}
        {turn.tools && turn.tools.length > 0 ? (
          <span className="mute">
            {" · "}
            {turn.tools.map((t) => t.name ?? "tool").join(", ")}
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
            tool: <span className="mono">{tool.name ?? "?"}</span>
            {tool.is_error ? <span className="hot"> (error)</span> : null}
            {tool.result_chars ? <span className="mute"> · {tool.result_chars} chars</span> : null}
          </summary>
          {tool.input ? (
            <pre className="tool-input">{JSON.stringify(tool.input, null, 2)}</pre>
          ) : null}
          {tool.result_preview ? <pre className="tool-result">{tool.result_preview}</pre> : null}
        </details>
      ))}
    </details>
  );
}
