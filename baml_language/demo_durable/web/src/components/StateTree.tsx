import { useEffect, useState } from "react";
import type { Api } from "../api";
import { formatBytes, formatClock, formatCount } from "../format";
import type { DumpValue, Run, StateDump } from "../protocol";
import { inlineSummary, orderedThreads, shapeOf } from "../stateValue";
import type { SnapshotRef } from "../timeline/Timeline";
import type { SourceFocus } from "./SourceView";

interface Props {
  api: Api;
  run: Run | null;
  /** The snapshot selected on the timeline. It wins over the latest snapshot of a paused run. */
  selectedSnapshot: SnapshotRef | null;
  onSelectSnapshot(snapshot: SnapshotRef | null): void;
  onFocusSource(focus: SourceFocus): void;
  /**
   * The dump that this panel loaded, or `null` while none is loaded. The
   * timeline uses it to name the top user frame of a snapshot marker
   * (contract section 10.3).
   */
  onStateLoaded?(state: { site: Run["site"]; run: string; n: number; dump: StateDump } | null): void;
}

/** The panel head has room for this many snapshot chips. Earlier snapshots stay selectable on the timeline. */
const MAX_CHIPS = 5;

/** The colored text of a value next to its name: a pill for a future or a cancel token, the variant of an enum, a one-line summary of an instance. */
function ValueText({ value, declared }: { value: DumpValue; declared?: string | null }) {
  const shape = shapeOf(value);
  switch (shape.shape) {
    case "future":
      return (
        <span className="val" data-kind="future" data-testid="state-future" data-future={shape.state ?? "unknown"}>
          {/* The declared type of a local already says `Future<Quote>`. */}
          {!(declared && declared.startsWith("Future")) && <><span className="type">{shape.type ?? "Future"}</span>{" "}</>}
          {shape.id !== null && <span className="muted" title={`future #${shape.id}. The thread that settles it names the same id.`}>#{shape.id} </span>}
          <span className="pill" data-future={shape.state ?? "unknown"}>{shape.state ?? "unknown"}</span>
          {shape.settled && <span className="muted"> {shape.settled.key === "error" || shape.settled.key === "panic" ? "error" : "→"} </span>}
          {shape.settled && <span className="val" data-kind={shape.settled.value.kind}>{inlineSummary(shape.settled.value, 48, 1)}</span>}
        </span>
      );
    case "cancel_token":
      return (
        <span className="val" data-kind="cancel_token" data-testid="state-cancel-token">
          {!(declared && declared.endsWith("CancelToken")) && <><span className="type">CancelToken</span>{" "}</>}
          <span className="pill" data-token={shape.cancelled === null ? "unknown" : shape.cancelled ? "cancelled" : "armed"}>
            {shape.cancelled === null ? "unknown" : shape.cancelled ? "cancelled" : "armed"}
          </span>
          {shape.sources > 0 && <span className="muted"> any of {shape.sources}</span>}
        </span>
      );
    case "task_group":
      return <span className="val" data-kind="task_group"><span className="type">TaskGroup</span> {shape.summary}</span>;
    case "enum":
      return (
        <span className="val" data-kind="enum" data-testid="state-enum">
          <span className="muted">{shape.enumName}.</span><span className="variant">{shape.variant}</span>
        </span>
      );
    case "map":
    case "array":
    case "instance":
      return <span className="val" data-kind={value.kind} data-testid={`state-${shape.shape}`}>{inlineSummary(value)}</span>;
    case "plain":
      return <span className="val" data-kind={value.kind}>{value.preview}</span>;
  }
}

function Value({ name, type, value, depth, mapKey }: { name: string; type?: string | null; value: DumpValue; depth: number; mapKey?: boolean }) {
  const shape = shapeOf(value);
  const head = (
    <>
      <span className={mapKey ? "key map-key" : "key"}>{mapKey ? JSON.stringify(name) : name}</span>
      {type ? <span className="type">: {type}</span> : null}
      <span className="muted">{mapKey ? " → " : " = "}</span>
      <ValueText value={value} declared={type} />
    </>
  );
  // The state of a future is in its pill. Its value, or its error, is the child worth opening.
  const children = shape.shape === "future" ? [...(shape.settled ? [shape.settled] : []), ...shape.rest] : (value.children ?? []);
  if (children.length === 0) return <div className="leaf">{head}</div>;
  return (
    <details open={depth < 1 && shape.shape !== "future"}>
      <summary>{head}</summary>
      {children.map((child, index) => (
        <Value key={`${child.key}:${index}`} name={child.key} value={child.value} depth={depth + 1} mapKey={shape.shape === "map"} />
      ))}
    </details>
  );
}

function Heap({ heap }: { heap: StateDump["heap"] }) {
  const kinds = Object.entries(heap.by_kind ?? {}).sort((a, b) => b[1].bytes - a[1].bytes);
  const max = Math.max(...kinds.map(([, totals]) => totals.bytes), 1);
  return (
    <table className="heap" data-testid="heap-table">
      <thead>
        <tr>
          <th>kind</th>
          <th>count</th>
          <th>bytes</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {kinds.map(([kind, totals]) => (
          <tr key={kind}>
            <td>{kind}</td>
            <td>{formatCount(totals.count)}</td>
            <td>{formatBytes(totals.bytes)}</td>
            <td className="bar"><i style={{ width: `${(totals.bytes / max) * 100}%` }} /></td>
          </tr>
        ))}
        <tr className="total">
          <td>total</td>
          <td>{formatCount(heap.objects)}</td>
          <td>{formatBytes(heap.bytes)}</td>
          <td />
        </tr>
      </tbody>
    </table>
  );
}

/**
 * The tooltip of a parked kind. The real worker records a thread that was
 * stopped between two instructions, or inside `await`, as `runnable`: the
 * resumed process executes its next instruction, which for an await is the
 * await itself.
 */
function parkedTitle(kind: string, detail: string): string {
  if (kind === "runnable") return "The resumed process executes the thread's next instruction. A thread that was inside `await` executes the await again and waits on the same future.";
  return detail;
}

/** "1 sleep, 4 remote_call": how the threads of a snapshot are parked. */
function parkedSummary(dump: StateDump): string {
  const counts = new Map<string, number>();
  for (const thread of dump.threads) counts.set(thread.parked.kind, (counts.get(thread.parked.kind) ?? 0) + 1);
  return [...counts].map(([kind, count]) => `${count} ${kind}`).join(", ");
}

export function StateTree({ api, run, selectedSnapshot, onSelectSnapshot, onFocusSource, onStateLoaded }: Props) {
  const latest = run && run.snapshots.length > 0 ? run.snapshots[run.snapshots.length - 1] : undefined;
  const target: SnapshotRef | null =
    selectedSnapshot ?? (run && (run.status === "paused" || run.status === "sleeping") && latest ? { site: run.site, run: run.id, n: latest.n } : null);
  const targetKey = target ? `${target.site}/${target.run}/${target.n}` : null;

  const [loaded, setLoaded] = useState<{ key: string; dump: StateDump } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setError(null);
    if (target === null || targetKey === null) return;
    let cancelled = false;
    api
      .snapshotState(target.site, target.run, target.n)
      .then((dump) => !cancelled && setLoaded({ key: targetKey, dump }))
      .catch((cause: unknown) => !cancelled && setError(cause instanceof Error ? cause.message : String(cause)));
    return () => {
      cancelled = true;
    };
    // `target` is derived from `targetKey`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, targetKey]);

  const dump = loaded !== null && loaded.key === targetKey ? loaded.dump : null;
  const site = target?.site;
  const run_id = target?.run;
  const n = target?.n;
  useEffect(() => {
    if (!onStateLoaded) return;
    onStateLoaded(dump === null || site === undefined || run_id === undefined || n === undefined ? null : { site, run: run_id, n, dump });
  }, [onStateLoaded, dump, site, run_id, n]);

  return (
    <section className="panel" data-testid="state-tree">
      <div className="panel-head">
        <span className="panel-title">State</span>
        {target && (
          <span className="mono clip">
            {target.site}/{target.run} · snapshot #{target.n}
            {selectedSnapshot ? " (from timeline)" : " (latest)"}
          </span>
        )}
        <span className="spacer" />
        {run && run.snapshots.length > MAX_CHIPS && (
          <span className="muted" title="Earlier snapshots are selectable on the timeline">…</span>
        )}
        {run?.snapshots.slice(-MAX_CHIPS).map((snapshot) => {
          const active = target !== null && target.run === run.id && target.n === snapshot.n;
          return (
            <button key={snapshot.n} className="btn small toggle" aria-pressed={active} title={`snapshot #${snapshot.n}, ${formatBytes(snapshot.bytes)}`}
              onClick={() => onSelectSnapshot(active && selectedSnapshot ? null : { site: run.site, run: run.id, n: snapshot.n })}>
              #{snapshot.n}
            </button>
          );
        })}
      </div>
      <div className="panel-body">
        {error && <div className="error-line" style={{ margin: 8 }}>{error}</div>}
        {target === null && (
          <div className="empty">
            The state tree shows a paused run. Pause the run, or select a snapshot marker on the timeline to inspect an
            earlier snapshot.
          </div>
        )}
        {target !== null && dump === null && !error && <div className="empty">Loading snapshot #{target.n}…</div>}
        {dump && (
          <div className="tree">
            <div className="muted mono" style={{ fontSize: 10.5, padding: "2px 0 4px" }} data-testid="state-summary">
              segment {dump.segment} · written {formatClock(dump.created_ts)} · {dump.threads.length} thread
              {dump.threads.length === 1 ? "" : "s"}
              {dump.threads.length > 1 && ` (${parkedSummary(dump)})`}
            </div>
            {orderedThreads(dump).map((thread, threadIndex) => (
              <details key={thread.thread} open={dump.threads.length <= 3 || threadIndex === 0} data-testid="state-thread" data-parked={thread.parked.kind}>
                <summary>
                  <b>thread {thread.thread}</b> <span className="muted">{thread.name}</span>{" "}
                  <span className="parked" data-parked={thread.parked.kind} title={parkedTitle(thread.parked.kind, thread.parked.detail)}>parked: {thread.parked.kind}</span>
                  {typeof thread.settles_future === "number" && (
                    <span className="muted" data-testid="settles-future" title={`The thread settles future #${thread.settles_future} when it ends${typeof thread.parent_thread === "number" ? `. Spawned by thread ${thread.parent_thread}` : ""}.`}>
                      {" "}settles #{thread.settles_future}
                    </span>
                  )}
                  {thread.cancelled === true && <span className="pill" data-future="cancelled" style={{ marginLeft: 5 }}>cancelled</span>}
                </summary>
                <div className="leaf muted mono" style={{ fontSize: 10.5, whiteSpace: "normal" }}>{thread.parked.detail}</div>
                {thread.frames.map((frame, index) => (
                  <details key={index} open={index === 0} data-testid="state-frame">
                    <summary>
                      <span className="key">{frame.function}</span>{" "}
                      <button className="frame-loc" onClick={(event) => { event.preventDefault(); onFocusSource({ file: frame.file, line: frame.line }); }}>
                        {frame.file}:{frame.line}
                      </button>
                      {index === 0 && <span className="muted"> innermost</span>}
                    </summary>
                    {frame.locals.length === 0 && <div className="leaf muted">no named locals</div>}
                    {frame.locals.map((local) => (
                      <Value key={local.name} name={local.name} type={local.type} value={local.value} depth={0} />
                    ))}
                  </details>
                ))}
              </details>
            ))}
            <h5>Heap by kind</h5>
            <Heap heap={dump.heap} />
          </div>
        )}
      </div>
    </section>
  );
}
