import { clock } from "../clock";
import { formatBytes, formatClock, formatCount, formatMs } from "../format";
import type { PauseStats, ResumeStats, Stat } from "../protocol";
import type { Connector, Gap, Marker, SegmentBar, ThreadBar, WaitInterval } from "./layout";
import { MARKER_LABELS } from "./MarkerGlyph";

const PROGRAM_SOURCE_LABELS: Record<string, string> = {
  store: "store (no compile)",
  compile: "compile",
};

const WAKE_REASONS: Record<string, string> = {
  timer: "the wake timer of the site server",
  manual: "a resume command",
  restart: "a restart of the site server, which found the run overdue or rebuilt its timer",
};

export type DetailTarget =
  | { kind: "marker"; item: Marker }
  | { kind: "segment"; item: SegmentBar }
  | { kind: "gap"; item: Gap }
  | { kind: "thread"; item: ThreadBar }
  | { kind: "wait"; item: WaitInterval }
  | { kind: "connector"; item: Connector };

function TimingRows({ rows }: { rows: [string, Stat][] }) {
  const max = Math.max(...rows.map(([, value]) => value ?? 0), 0.001);
  return (
    <>
      {rows.map(([label, value]) => (
        <tr key={label}>
          <td>{label}</td>
          <td>
            {formatMs(value)}
            <span className="meter" style={{ width: `${Math.max(((value ?? 0) / max) * 100, value === null ? 0 : 2)}%` }} />
          </td>
        </tr>
      ))}
    </>
  );
}

export function PauseStatsTable({ stats }: { stats: PauseStats }) {
  const ratio =
    typeof stats.raw_bytes === "number" && typeof stats.compressed_bytes === "number" && stats.raw_bytes > 0
      ? `${((stats.compressed_bytes / stats.raw_bytes) * 100).toFixed(0)}% of raw`
      : null;
  return (
    <table className="kv" data-testid="pause-stats">
      <tbody>
        <TimingRows
          rows={[
            ["pause latency", stats.pause_latency_ms],
            ["graph walk", stats.walk_ms],
            ["encode", stats.encode_ms],
            ["compress", stats.compress_ms],
            ["write", stats.write_ms],
          ]}
        />
        <tr className="total">
          <td>objects</td>
          <td>{formatCount(stats.objects)}</td>
        </tr>
        <tr>
          <td>raw</td>
          <td>{formatBytes(stats.raw_bytes)}</td>
        </tr>
        <tr>
          <td>compressed</td>
          <td>
            {formatBytes(stats.compressed_bytes)}
            {ratio && <span className="muted"> ({ratio})</span>}
          </td>
        </tr>
        <tr>
          <td>program</td>
          <td>{formatBytes(stats.program_bytes)}</td>
        </tr>
        <tr>
          <td>blocked attempts</td>
          <td>{formatCount(stats.blocked_attempts)}</td>
        </tr>
      </tbody>
    </table>
  );
}

export function ResumeStatsTable({ stats }: { stats: ResumeStats }) {
  return (
    <table className="kv" data-testid="resume-stats">
      <tbody>
        <TimingRows
          rows={[
            ["process start", stats.process_start_ms],
            ["program load", stats.program_load_ms],
            ["decode and allocate", stats.decode_ms],
          ]}
        />
        <tr className="total">
          <td>until first exec()</td>
          <td>{formatMs(stats.first_exec_ms)}</td>
        </tr>
        {stats.program_source != null && (
          <tr data-testid="program-source" data-source={stats.program_source}>
            <td>program from</td>
            <td>{PROGRAM_SOURCE_LABELS[stats.program_source] ?? stats.program_source}</td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

function Kv({ rows }: { rows: [string, string][] }) {
  return (
    <table className="kv">
      <tbody>
        {rows.map(([label, value]) => (
          <tr key={label}>
            <td>{label}</td>
            <td>{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const END_LABELS: Record<SegmentBar["endKind"], string> = {
  open: "process exists",
  paused: "paused: snapshot written, process exited",
  completed: "completed",
  failed: "failed",
  cancelled: "cancelled",
  lost: "process lost, no snapshot",
  killed: "process ended without a terminal event; the run continues from a snapshot",
};

export function TimelineDetail({ target, pinned }: { target: DetailTarget | null; pinned: boolean }) {
  if (target === null) {
    return (
      <div className="muted">
        Hover or select a marker, bar, gap, or connector. A snapshot marker shows its pause timings and loads its state
        into the state tree.
      </div>
    );
  }
  const where = (site: string, run: string, segment?: number): string =>
    `${site}/${run}${segment === undefined ? "" : ` seg ${segment}`}`;
  switch (target.kind) {
    case "marker": {
      const marker = target.item;
      return (
        <div data-testid="timeline-detail" data-kind={marker.kind}>
          <h4>
            {MARKER_LABELS[marker.kind]}
            {marker.kind === "snapshot" ? ` #${marker.n}` : ""}
            {pinned ? "" : " (hover)"}
          </h4>
          <div className="sub">
            {where(marker.site, marker.run, marker.kind === "fork" ? undefined : marker.segment)} at {formatClock(marker.t)}
          </div>
          {marker.kind === "snapshot" &&
            (marker.stats ? <PauseStatsTable stats={marker.stats} /> : <Kv rows={[["size", formatBytes(marker.bytes)]]} />)}
          {marker.kind === "snapshot" && marker.automatic && (
            <div className="muted">Automatic snapshot. No pause was requested, and the process kept running.</div>
          )}
          {marker.kind === "snapshot" && marker.wake && (
            <>
              <Kv
                rows={[
                  ["sleep remaining", formatMs(marker.wake.remainingMs)],
                  ["wake at", formatClock(marker.wake.wakeAt)],
                ]}
              />
              <div className="muted">
                The run suspended itself. Every thread was parked in a wait that can be issued again, and the earliest
                sleep had this long to go. The process exited, and a timer of the site server resumes the run.
              </div>
            </>
          )}
          {marker.kind === "wake" && (
            <>
              <Kv
                rows={[
                  ["woken by", marker.reason],
                  ["timer set for", marker.wakeAt === null ? "n/a" : formatClock(marker.wakeAt)],
                  ["timer error", marker.wakeAt === null ? "n/a" : formatMs(marker.t - marker.wakeAt)],
                  ["without a process for", formatMs(marker.sleptMs)],
                ]}
              />
              <div className="muted">
                The run was resumed by {WAKE_REASONS[marker.reason] ?? marker.reason}. A new process restores the snapshot. A
                sleep whose deadline has passed completes at once.
              </div>
            </>
          )}
          {marker.kind === "cancel" && (
            <>
              <Kv
                rows={[
                  ["call", marker.callId],
                  ...(marker.function ? ([["function", marker.function]] as [string, string][]) : []),
                  ["thread", marker.thread === null ? "n/a" : String(marker.thread)],
                  ["child", marker.childRun === null ? "not dispatched" : `${marker.childSite}/${marker.childRun}`],
                ]}
              />
              <div className="muted">
                {marker.source === "worker"
                  ? "The thread that waited on this call was cancelled: a race loser, a timeout, a cancel token, or Future.cancel. The site server cancels the child run on its machine, and a late result is discarded."
                  : "The run ended, and its site cancelled the remote children that were still running."}
              </div>
            </>
          )}
          {marker.kind === "fork" && (
            <Kv
              rows={[
                ["forked from", `${marker.site}/${marker.fromRun}`],
                ["snapshot", `#${marker.n}`],
              ]}
            />
          )}
          {marker.kind === "resume" && <ResumeStatsTable stats={marker.stats} />}
          {marker.kind === "pause_request" &&
            (marker.waitingOn.length > 0 ? (
              <>
                <div className="muted">The pause waits on:</div>
                <div className="crumbs" style={{ flexDirection: "column" }}>
                  {marker.waitingOn.map((operation) => (
                    <div key={operation}>{operation}</div>
                  ))}
                </div>
              </>
            ) : (
              <div className="muted">No operation in flight delayed this pause.</div>
            ))}
          {marker.kind === "blocked" && (
            <>
              <div>{marker.reason}</div>
              <div className="crumbs">
                {marker.path.map((part, index) => (
                  <span key={index}>{part}</span>
                ))}
              </div>
            </>
          )}
        </div>
      );
    }
    case "segment": {
      const bar = target.item;
      return (
        <div data-testid="timeline-detail" data-kind="segment">
          <h4>
            segment {bar.segment} · pid {bar.pid ?? "?"}
          </h4>
          <div className="sub">{where(bar.site, bar.run)}</div>
          <Kv
            rows={[
              ["mode", bar.mode ?? "unknown"],
              ...(bar.programHash ? ([["program", bar.programHash.slice(0, 12)]] as [string, string][]) : []),
              ["start", formatClock(bar.start)],
              ["duration", formatMs(bar.end - bar.start)],
              ["end", END_LABELS[bar.endKind]],
            ]}
          />
        </div>
      );
    }
    case "gap": {
      const gap = target.item;
      if (gap.kind === "sleeping") {
        return (
          <div data-testid="timeline-detail" data-kind="gap" data-gap-kind="sleeping">
            <h4>sleeping, no process</h4>
            <div className="sub">{where(gap.site, gap.run)}</div>
            <Kv
              rows={[
                ["state lives in", gap.snapshotN === null ? "snapshot" : `snapshot #${gap.snapshotN}`],
                ["snapshot size", formatBytes(gap.bytes)],
                ["wake at", gap.wakeAt === null ? "n/a" : formatClock(gap.wakeAt)],
                gap.open
                  ? ["wakes in", gap.wakeAt === null ? "n/a" : formatMs(Math.max(gap.wakeAt - clock.now(), 0))]
                  : ["slept for", formatMs(gap.end - gap.start)],
                ...(gap.wakeReason ? ([["woken by", gap.wakeReason]] as [string, string][]) : []),
              ]}
            />
            <div className="muted">
              The run suspended itself for a sleep. No process and no memory are held while it sleeps. Remote children keep
              running on their machines, and their results are stored until the wake.
            </div>
          </div>
        );
      }
      return (
        <div data-testid="timeline-detail" data-kind="gap" data-gap-kind={gap.kind}>
          <h4>{gap.fork ? "fork, no process yet" : "no process"}</h4>
          <div className="sub">{where(gap.site, gap.run)}</div>
          <Kv
            rows={[
              ["state lives in", gap.snapshotN === null ? "snapshot" : `snapshot #${gap.snapshotN}`],
              ["snapshot size", formatBytes(gap.bytes)],
              [gap.open ? "paused for" : "duration", gap.open ? "not resumed yet" : formatMs(gap.end - gap.start)],
            ]}
          />
        </div>
      );
    }
    case "thread": {
      const thread = target.item;
      return (
        <div data-testid="timeline-detail" data-kind="thread">
          <h4>spawned thread {thread.thread}</h4>
          <div className="sub">{where(thread.site, thread.run, thread.segment)}</div>
          <Kv
            rows={[
              ["parent thread", String(thread.parentThread ?? "none")],
              [thread.continued || thread.continues ? "alive in this segment" : "lifetime", formatMs(thread.end - thread.start)],
              ...(thread.continued ? ([["restored", "from the snapshot of an earlier segment"]] as [string, string][]) : []),
              ...(thread.continues ? ([["continues", "in a later segment, from the snapshot"]] as [string, string][]) : []),
            ]}
          />
        </div>
      );
    }
    case "wait": {
      const wait = target.item;
      return (
        <div data-testid="timeline-detail" data-kind="wait">
          <h4>waiting on a remote call</h4>
          <div className="sub">{where(wait.site, wait.run, wait.segment)}</div>
          <Kv
            rows={[
              ["call", wait.callId],
              ["thread", String(wait.thread)],
              ["waited in this segment", formatMs(wait.end - wait.start)],
              ...(wait.cancelled ? ([["ended by", "cancellation"]] as [string, string][]) : []),
            ]}
          />
        </div>
      );
    }
    case "connector": {
      const connector = target.item;
      const titles = { call: "remote call", return: "remote result", migration: "migration", fork: "fork", cancel: "remote cancel" } as const;
      return (
        <div data-testid="timeline-detail" data-kind={connector.kind}>
          <h4>{titles[connector.kind]}</h4>
          <div className="sub">{connector.label}</div>
          <Kv
            rows={[
              ...(connector.callId ? ([["call", connector.callId]] as [string, string][]) : []),
              ["from", `${connector.from.site} at ${formatClock(connector.from.t)}`],
              ["to", `${connector.to.site} at ${formatClock(connector.to.t)}`],
              connector.kind === "cancel" && connector.pending
                ? ["child", "still running, cancel in flight"]
                : [
                    connector.kind === "fork" ? "snapshot age at the fork" : connector.kind === "cancel" ? "until the child's process ended" : "latency",
                    formatMs(connector.to.t - connector.from.t),
                  ],
              ...(connector.kind === "return"
                ? ([
                    ["result", connector.ok === false ? "error" : "ok"],
                    ["delivery", connector.pending ? "stored until the resume" : "delivered to the process"],
                  ] as [string, string][])
                : []),
            ]}
          />
        </div>
      );
    }
  }
}
