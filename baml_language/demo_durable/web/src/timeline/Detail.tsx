/**
 * The detail panel of the timeline. It names the cause of the selected object
 * (contract section 10.3, computed in `cause.ts`), offers its source location
 * as a link, and adds the timing tables that a snapshot and a resume carry.
 */

import { formatBytes, formatCount, formatMs } from "../format";
import type { PauseStats, ResumeStats, Stat } from "../protocol";
import { causeOf, locationLabel, type Cause, type CauseContext, type CauseLocation, type DetailTarget } from "./cause";
import type { Connector, Marker } from "./layout";

export type { DetailTarget } from "./cause";

const PROGRAM_SOURCE_LABELS: Record<string, string> = {
  store: "store (no compile)",
  compile: "compile",
};

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


/** The kind of object that the panel describes, kept on `data-kind` for tests and styling. */
function kindOf(target: DetailTarget): string {
  switch (target.kind) {
    case "marker":
      return target.item.kind;
    case "connector":
      return target.item.kind;
    default:
      return target.kind;
  }
}

/** The `<site>/<run>` line under the heading. */
function whereOf(target: DetailTarget): string {
  const item = target.item;
  const segment =
    target.kind === "connector" || (target.kind === "marker" && (item as Marker).kind === "fork")
      ? null
      : (item as { segment?: number }).segment ?? null;
  if (target.kind === "connector") {
    const connector = item as Connector;
    return `${connector.from.site}/${connector.from.run} → ${connector.to.site}/${connector.to.run}`;
  }
  const located = item as { site: string; run: string };
  return `${located.site}/${located.run}${segment === null || segment === 0 ? "" : ` seg ${segment}`}`;
}

/**
 * The source location of a cause, as a link. Following it opens the file in
 * the source view and highlights the line (contract section 10.3).
 */
function LocationLink({ location, onOpenSource }: { location: CauseLocation; onOpenSource?: (focus: SourceFocusRequest) => void }) {
  return (
    <div className="cause-loc-row">
      <button
        className="cause-loc"
        data-testid="cause-location"
        data-file={location.file}
        data-line={location.line}
        data-approximate={location.approximate ? "true" : "false"}
        title={`Open ${location.file}:${location.line} in the source view`}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onOpenSource?.({ file: location.file, line: location.line, site: location.site, origin: "cause", what: location.what });
        }}
      >
        {locationLabel(location)}
      </button>
      <span className="muted" data-testid="cause-location-what">
        {location.what}
        {location.approximate ? " · approximate" : ""}
      </span>
    </div>
  );
}

/** What the panel asks the app to show in the source view. */
export interface SourceFocusRequest {
  file: string;
  line: number;
  site: string;
  origin: "cause";
  what: string;
}

export interface DetailProps {
  target: DetailTarget | null;
  pinned: boolean;
  /** Run records and event lists, so the panel can name the cause of the object. */
  context: CauseContext;
  now: number;
  onOpenSource?: (focus: SourceFocusRequest) => void;
}

export function TimelineDetail({ target, pinned, context, now, onOpenSource }: DetailProps) {
  if (target === null) {
    return (
      <div className="muted">
        Hover or select an arrow, a bar, a gap, or a marker. The panel says what made it happen and links to the line of
        the program it came from.
      </div>
    );
  }
  const cause: Cause = causeOf(target, context, now);
  const marker = target.kind === "marker" ? target.item : null;
  return (
    <div data-testid="timeline-detail" data-kind={kindOf(target)} data-gap-kind={target.kind === "gap" ? target.item.kind : undefined}>
      <h4>
        {cause.title}
        {pinned ? "" : " (hover)"}
      </h4>
      <div className="sub">{whereOf(target)}</div>
      <p className="cause" data-testid="cause-sentence">
        {cause.sentence}
      </p>
      {cause.location ? (
        <LocationLink location={cause.location} onOpenSource={onOpenSource} />
      ) : cause.missing ? (
        <div className="muted cause-missing" data-testid="cause-missing">
          No source location: {cause.missing}
        </div>
      ) : null}
      {cause.rows.length > 0 && <Kv rows={cause.rows} />}
      {cause.note && <div className="muted">{cause.note}</div>}
      {marker?.kind === "snapshot" &&
        (marker.stats ? <PauseStatsTable stats={marker.stats} /> : <Kv rows={[["size", formatBytes(marker.bytes)]]} />)}
      {marker?.kind === "resume" && <ResumeStatsTable stats={marker.stats} />}
      {marker?.kind === "pause_request" && marker.waitingOn.length > 0 && (
        <>
          <div className="muted">The pause waits on:</div>
          <div className="crumbs" style={{ flexDirection: "column" }}>
            {marker.waitingOn.map((operation) => (
              <div key={operation}>{operation}</div>
            ))}
          </div>
        </>
      )}
      {marker?.kind === "blocked" && marker.path.length > 0 && (
        <div className="crumbs">
          {marker.path.map((part, index) => (
            <span key={index}>{part}</span>
          ))}
        </div>
      )}
    </div>
  );
}
