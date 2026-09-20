import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { formatBytes, formatClock, formatMs } from "../format";
import type { Site, SiteEntry } from "../protocol";
import { splitRunKey, type AppState, type RunKey, type TimelineEvent } from "../state";
import { Split } from "./Split";

interface LogLine {
  id: string;
  ts: number;
  run: string;
  pid: number | null;
  stream: "stdout" | "stderr" | "worker_stderr" | "sys";
  text: string;
}

const STREAM_LABELS: Record<LogLine["stream"], string> = {
  stdout: "out",
  stderr: "err",
  worker_stderr: "werr",
  sys: "sys",
};

function preview(value: unknown): string {
  const text = JSON.stringify(value) ?? "null";
  return text.length > 120 ? `${text.slice(0, 117)}...` : text;
}

/** The text of a lifecycle event in the log lane, or `null` for an event that the lane does not show. */
export function systemLine(event: TimelineEvent): string | null {
  switch (event.type) {
    case "hello":
      return `process ${event.pid} started (${event.mode}) ${event.function}${event.durable ? " [durable]" : ""}`;
    case "thread_started":
      return event.parent_thread === null ? null : `thread ${event.thread} spawned by thread ${event.parent_thread}`;
    case "thread_ended":
      return null;
    case "remote_call":
      return `remote call ${event.function}(${preview(event.args)}) as ${event.call_id}`;
    case "remote_dispatched":
      return `dispatched ${event.call_id} to ${event.child_site}/${event.child_run}`;
    case "remote_returned":
      return `result of ${event.call_id} arrived from ${event.child_site}/${event.child_run} (${event.ok ? "ok" : "error"})`;
    case "remote_result_received":
      return `thread ${event.thread} received the result of ${event.call_id}`;
    case "pausing":
      return `pause delayed, waiting on ${event.waiting_on.join(", ") || "operations in flight"}`;
    case "blocked":
      return `snapshot blocked: ${event.reason} (${event.path.join(" › ")})`;
    case "paused":
      if (event.wake) {
        return `suspended itself for a sleep with ${formatMs(event.wake.remaining_ms)} to go: snapshot written, ${formatBytes(event.stats?.compressed_bytes)}. Process ${event.pid} exits`;
      }
      return `snapshot written, ${formatBytes(event.stats?.compressed_bytes)} after ${formatMs(event.stats?.pause_latency_ms)}. Process ${event.pid} exits`;
    case "sleep_scheduled":
      return `sleeping without a process. The wake timer is set for ${formatClock(event.wake_at)}`;
    case "woken":
      return `woken by ${event.reason === "timer" ? "the wake timer" : event.reason === "manual" ? "a resume command" : event.reason === "restart" ? "a restart of the site server" : event.reason}`;
    case "remote_cancel":
      return `thread ${event.thread} was cancelled while it waited on ${event.call_id}. The run no longer waits on the call`;
    case "remote_cancelled":
      return `cancelled ${event.call_id}: asked ${event.child_site} to cancel ${event.child_run}`;
    case "snapshot":
      return `automatic snapshot written, ${formatBytes(event.stats?.compressed_bytes)}. The process keeps running`;
    case "resumed":
      return `resumed from snapshot, first exec() after ${formatMs(event.stats?.first_exec_ms)}${
        event.stats?.program_source === "store" ? ". The program came from the program store, without a compile" : event.stats?.program_source === "compile" ? ". The program was compiled" : ""
      }`;
    case "completed":
      return `completed: ${preview(event.value)}`;
    case "failed":
      return `failed: ${event.error}`;
    case "migrated_out":
      return `migrated to ${event.to_site}`;
    case "cancelled":
      return `run cancelled. Process ${event.pid} exits`;
    case "migrated_in":
      return `imported from ${event.from_site}`;
    case "forked":
      return `forked from ${event.from_run} at snapshot #${event.n}`;
    case "worker_exit": {
      // A clean exit follows a terminal event, which has its own line.
      const clean = event.signal === null && [0, 1, 75, 130].includes(event.exit_code);
      if (clean) return null;
      const how = event.signal === null ? `exit code ${event.exit_code}` : `signal ${event.signal}`;
      return `process ${event.pid ?? "?"} ended without a terminal event (${how}). The run is ${event.status}`;
    }
    case "ui_status":
      // The `worker_exit` and `cancelled` events report the end of a process (section 7).
      return null;
    default:
      return null;
  }
}

function linesOf(events: AppState["events"], site: Site, only: ReadonlySet<RunKey> | null): LogLine[] {
  const lines: LogLine[] = [];
  for (const [key, list] of Object.entries(events) as [RunKey, TimelineEvent[]][]) {
    if (splitRunKey(key).site !== site) continue;
    if (only !== null && !only.has(key)) continue;
    list.forEach((event, index) => {
      const pid = "pid" in event && typeof event.pid === "number" ? event.pid : null;
      if (event.type === "log") {
        lines.push({ id: `${key}:${index}`, ts: event.ts, run: event.run, pid, stream: event.stream, text: event.text });
        return;
      }
      const text = systemLine(event);
      if (text !== null) lines.push({ id: `${key}:${index}`, ts: event.ts, run: event.run, pid, stream: "sys", text });
    });
  }
  return lines.sort((a, b) => a.ts - b.ts);
}

function Lane({ site, lines, selectedRun }: { site: Site; lines: LogLine[]; selectedRun: string | null }) {
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const stick = useRef(true);
  const [held, setHeld] = useState(false);
  const [seen, setSeen] = useState(0);

  const onScroll = (): void => {
    const body = bodyRef.current;
    if (!body) return;
    const atBottom = body.scrollHeight - body.scrollTop - body.clientHeight < 12;
    stick.current = atBottom;
    setHeld(!atBottom);
    if (atBottom) setSeen(lines.length);
  };

  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (body && stick.current) {
      body.scrollTop = body.scrollHeight;
      setSeen(lines.length);
    }
  }, [lines.length]);

  // A shorter list means another run tree. Start at the bottom again.
  useEffect(() => {
    if (lines.length < seen) {
      stick.current = true;
      setHeld(false);
    }
  }, [lines.length, seen]);

  const unseen = Math.max(lines.length - seen, 0);
  return (
    <section className="panel log-lane" data-site={site} data-testid={`log-lane-${site}`}>
      <div className="panel-head">
        <i className="site-dot" data-site={site} />
        <span className="panel-title">{site} log</span>
        <span className="spacer" />
        <span className="muted">{held ? "scroll paused" : "following"}</span>
        <span className="num">{lines.length}</span>
      </div>
      <div className="panel-body" ref={bodyRef} onScroll={onScroll}>
        {lines.length === 0 ? (
          <div className="empty">No output on {site}.</div>
        ) : (
          <div className="log-lines">
            {lines.map((line) => (
              <div key={line.id} className="log-line" data-stream={line.stream} data-site={site} data-selected={line.run === selectedRun}>
                <span className="t">{formatClock(line.ts)}</span>
                <span className="run" title={line.run}>{line.run}</span>
                <span className="pid">{line.pid ?? "—"}</span>
                <span className="stream">{STREAM_LABELS[line.stream] ?? line.stream}</span>
                <span className="text" title={line.stream === "sys" ? line.text : undefined}>{line.text}</span>
              </div>
            ))}
          </div>
        )}
      </div>
      {held && (
        <button className="log-jump" onClick={() => {
          const body = bodyRef.current;
          stick.current = true;
          setHeld(false);
          if (body) body.scrollTop = body.scrollHeight;
        }}>
          ↓ {unseen > 0 ? `${unseen} new line${unseen === 1 ? "" : "s"}` : "Jump to latest"}
        </button>
      )}
    </section>
  );
}

interface Props {
  /** The site registry. One lane per site, in registry order. */
  sites: readonly SiteEntry[];
  events: AppState["events"];
  tree: readonly RunKey[];
  selected: RunKey | null;
}

export function LogLanes({ sites, events, tree, selected }: Props) {
  const [treeOnly, setTreeOnly] = useState(true);
  const filter = useMemo(() => (treeOnly && tree.length > 0 ? new Set(tree) : null), [treeOnly, tree]);
  const names = useMemo(() => sites.map((site) => site.name), [sites]);
  const lines = useMemo(
    () => Object.fromEntries(names.map((site) => [site, linesOf(events, site, filter)])) as Record<Site, LogLine[]>,
    [names, events, filter],
  );
  const selectedRun = selected ? splitRunKey(selected).id : null;
  const share = 100 / Math.max(names.length, 1);
  return (
    <div className="logs-wrap" style={{ display: "flex", flexDirection: "column", minHeight: 0, minWidth: 0, gap: 4 }}>
      <div className="logs" style={{ flex: 1 }}>
        {/* The pane set follows the registry. The key remounts the group when the
            set changes, and the pane ids name the sites, so a layout that was
            saved for another set of lanes is never applied to this one. */}
        <Split key={names.join(",")} id="logs" orientation="horizontal" panes={names.map((site) => ({
          id: `logpane-${site}`,
          defaultSize: share.toFixed(2),
          minSize: Math.min(15, share / 2).toFixed(2),
          content: <Lane site={site} lines={lines[site] ?? []} selectedRun={selectedRun} />,
        }))} />
      </div>
      <label className="muted" style={{ display: "flex", gap: 5, alignItems: "center", fontSize: 11, flex: "none" }}>
        <input type="checkbox" checked={treeOnly} onChange={(event) => setTreeOnly(event.target.checked)} data-testid="log-filter" />
        Show only the selected run tree in the log lanes
      </label>
    </div>
  );
}
