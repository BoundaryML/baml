"use client";

import Link from "next/link";

import type { LiveState } from "../../lib/data";
import { issueStatusLabel } from "../../lib/data";
import { ago } from "../../lib/format";
import { usePolledState } from "../../live-dashboard";

const TONE: Record<string, string> = {
  queued: "mute", running: "ok",
  done: "completed", failed: "failed", deduping: "partial", success: "completed", partial: "partial",
  open: "partial", confirmed: "timeout", approved: "completed", fixing: "partial",
  cursor: "cursor", closed: "mute", rejected: "mute",
};
const ACTIVE = new Set(["running", "deduping", "syncing", "building"]);

const pill = (v: string) => <span className={`statpill ${TONE[v] ?? ""}`}>{v}</span>;

/**
 * Client component rendering a live table view of tasks, trophies, or issues with
 * status pills and links to run details. Polls for fresh state and lets the user
 * pause/resume the live updates.
 * @param table - which table to render ("tasks", "trophies", or "issues")
 * @param initial - the server-rendered LiveState used to seed live polling
 * @returns the selected table view
 */
export default function DbView({ table, initial }: { table: string; initial: LiveState }) {
  const { s, live, setLive } = usePolledState(initial);
  const now = Date.now();

  let head: React.ReactNode = null;
  let rows: React.ReactNode[] = [];

  if (table === "tasks") {
    const data = [...s.tasks].sort((a, b) =>
      (ACTIVE.has(b.status) ? 1 : 0) - (ACTIVE.has(a.status) ? 1 : 0) || b.createdAt - a.createdAt);
    head = <tr><th>status</th><th>source</th><th>prompt</th><th>report</th><th>worker</th><th className="r">age</th></tr>;
    rows = data.map((t) => (
      <tr key={t._id} className="runrow">
        <td>{pill(t.status)}{ACTIVE.has(t.status) ? <span className="pulse on" style={{ marginLeft: 6 }} /> : null}</td>
        <td className="mono mute">{t.source}</td>
        <td>{t.prompt}</td>
        <td>{t.reportId ? <Link href={`/runs/${t.reportId}`}>trophy →</Link> : <span className="mute">—</span>}</td>
        <td className="mono mute">{(t.claimedBy ?? "").slice(0, 16)}</td>
        <td className="r mono mute">{ago(now - (t.claimedAt ?? t.createdAt))}</td>
      </tr>
    ));
  } else if (table === "trophies") {
    head = <tr><th>outcome</th><th>task</th><th>src</th><th className="r">turns</th>
      <th className="r">api</th><th className="r">tokens</th><th className="r">cost</th><th className="r">issues</th></tr>;
    rows = s.runs.map((r) => (
      <tr key={r.trophyId} className="runrow">
        <td>{pill(r.outcome)}</td>
        <td><Link href={`/runs/${r.trophyId}`}>{r.prompt}</Link></td>
        <td className="mono mute">{r.source}</td>
        <td className="r mono">{r.turns ?? "-"}</td>
        <td className="r mono">{r.apiCalls ?? "-"}</td>
        <td className="r mono">{r.outputTokens ?? "-"}</td>
        <td className="r mono">${(r.costUsd ?? 0).toFixed(2)}</td>
        <td className="r mono">{r.findings || ""}</td>
      </tr>
    ));
  } else {
    head = <tr><th>kind</th><th>status</th><th>title</th><th>reports</th></tr>;
    rows = s.issues.map((i) => (
      <tr key={i._id} className="runrow">
        <td>{pill(i.kind)}</td>
        <td>{pill(issueStatusLabel(i))}</td>
        <td>{i.title}</td>
        <td className="mono">
          {(i.evidence ?? []).filter((e) => e.trophyId).map((e, idx) => (
            <Link key={idx} href={`/runs/${e.trophyId}${e.call_index != null ? `?call=${e.call_index}` : ""}`}
              style={{ marginRight: 8 }}>
              report{e.call_index != null ? `·c${e.call_index}` : ""}
            </Link>
          ))}
          {(i.evidence ?? []).length === 0 ? <span className="mute">—</span> : null}
        </td>
      </tr>
    ));
  }

  return (
    <div>
      <header className="page">
        <p style={{ marginBottom: 6 }}><Link href="/" className="back-link">← graph</Link></p>
        <h1>{table} <span className={`pulse ${live ? "on" : ""}`} /></h1>
        <p className="mute" style={{ fontSize: 13 }}>
          {rows.length} rows ·{" "}
          <button className="linkbtn" onClick={() => setLive((v) => !v)}>{live ? "live ⏸" : "paused ▶"}</button>
        </p>
      </header>
      {rows.length === 0 ? <p className="mute">empty.</p> : (
        <table className="runtable">
          <thead>{head}</thead>
          <tbody>{rows}</tbody>
        </table>
      )}
    </div>
  );
}
