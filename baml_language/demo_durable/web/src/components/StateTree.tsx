import { useEffect, useState } from "react";
import type { Api } from "../api";
import { formatBytes, formatClock, formatCount } from "../format";
import type { DumpValue, Run, StateDump } from "../protocol";
import type { SnapshotRef } from "../timeline/Timeline";
import type { SourceFocus } from "./SourceView";

interface Props {
  api: Api;
  run: Run | null;
  /** The snapshot selected on the timeline. It wins over the latest snapshot of a paused run. */
  selectedSnapshot: SnapshotRef | null;
  onSelectSnapshot(snapshot: SnapshotRef | null): void;
  onFocusSource(focus: SourceFocus): void;
}

/** The panel head has room for this many snapshot chips. Earlier snapshots stay selectable on the timeline. */
const MAX_CHIPS = 5;

function Value({ name, type, value, depth }: { name: string; type?: string | null; value: DumpValue; depth: number }) {
  const head = (
    <>
      <span className="key">{name}</span>
      {type ? <span className="type">: {type}</span> : null}
      <span className="muted"> = </span>
      <span className="val" data-kind={value.kind}>
        {value.class && value.kind === "instance" && !value.preview.startsWith(value.class) ? `${value.class} ` : ""}
        {value.preview}
      </span>
    </>
  );
  if (!value.children || value.children.length === 0) return <div className="leaf">{head}</div>;
  return (
    <details open={depth < 1}>
      <summary>{head}</summary>
      {value.children.map((child, index) => (
        <Value key={`${child.key}:${index}`} name={child.key} value={child.value} depth={depth + 1} />
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

export function StateTree({ api, run, selectedSnapshot, onSelectSnapshot, onFocusSource }: Props) {
  const latest = run && run.snapshots.length > 0 ? run.snapshots[run.snapshots.length - 1] : undefined;
  const target: SnapshotRef | null =
    selectedSnapshot ?? (run && run.status === "paused" && latest ? { site: run.site, run: run.id, n: latest.n } : null);
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
            <div className="muted mono" style={{ fontSize: 10.5, padding: "2px 0 4px" }}>
              segment {dump.segment} · written {formatClock(dump.created_ts)} · {dump.threads.length} thread
              {dump.threads.length === 1 ? "" : "s"}
            </div>
            {dump.threads.map((thread) => (
              <details key={thread.thread} open data-testid="state-thread">
                <summary>
                  <b>thread {thread.thread}</b> <span className="muted">{thread.name}</span>{" "}
                  <span className="parked" title={thread.parked.detail}>parked: {thread.parked.kind}</span>
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
