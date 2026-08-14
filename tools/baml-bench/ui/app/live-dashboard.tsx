"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

import type { LiveState } from "./lib/data";
import { ago } from "./lib/format";

const STAGE: Record<string, string> = {
  worker: "ok", dedup: "warn", "notion-sync": "link", "baml-build": "mute",
};
const TONE: Record<string, string> = {
  queued: "mute", running: "ok",
  done: "ok", failed: "hot", deduping: "warn", success: "ok", partial: "warn",
  open: "warn", confirmed: "link", approved: "ok", fixing: "warn",
  cursor: "link", closed: "mute", rejected: "mute", ready: "ok", building: "warn",
};

/**
 * Hook that polls /api/state every 3s and exposes the latest live snapshot,
 * shared by the graph, db, and live-dashboard views.
 * @param initial - the server-rendered LiveState to seed before polling starts
 * @returns the current state `s`, a `live` toggle, and `setLive` to pause/resume polling
 */
export function usePolledState(initial: LiveState) {
  const [s, setS] = useState<LiveState>(initial);
  const [live, setLive] = useState(true);
  useEffect(() => {
    if (!live) return;
    let alive = true;
    const id = setInterval(async () => {
      try {
        const r = await fetch("/api/state", { cache: "no-store" });
        if (alive && r.ok) setS(await r.json());
      } catch { /* keep last */ }
    }, 3000);
    return () => { alive = false; clearInterval(id); };
  }, [live]);
  return { s, live, setLive };
}

const cnt = (o: Record<string, number>) => Object.values(o).reduce((a, b) => a + b, 0);

/**
 * A clickable database node linking to /db/[table], showing the total row count
 * and a status breakdown of chips. Renders an "empty" hint when there are no rows.
 * @param name - the display label for the node
 * @param table - the table slug used in the /db/[table] link
 * @param breakdown - map of status to count, rendered as sorted chips
 */
function DbNode({ name, table, breakdown }:
  { name: string; table: string; breakdown: Record<string, number> }) {
  const parts = Object.entries(breakdown).sort((a, b) => b[1] - a[1]);
  return (
    <Link href={`/db/${table}`} className="dbnode">
      <div className="dbnode-name">{name}</div>
      <div className="dbnode-num">{cnt(breakdown)}</div>
      <div className="dbnode-rows">
        {parts.length === 0 ? <span className="mute">empty</span> :
          parts.map(([k, v]) => (
            <span key={k} className="chip"><span className={`dot dot-${TONE[k] ?? "mute"}`} />{v}&nbsp;{k}</span>
          ))}
      </div>
    </Link>
  );
}

/**
 * Client component rendering the live dashboard: the tasks -> trophies -> issues
 * pipeline flow with in-flight work, and a recent-runs table linking to each trophy.
 * @param initial - the server-rendered LiveState used to seed live polling
 * @returns the live dashboard view
 */
export default function LiveDashboard({ initial }: { initial: LiveState }) {
  const { s, live, setLive } = usePolledState(initial);
  const readyBuild = (s.builds ?? []).find((b) => b.status === "ready");
  const now = Date.now();
  const inflightTasks = s.inflight.filter((f) => f.kind === "task");

  return (
    <div>
      <header className="page">
        <h1>baml-bench <span className={`pulse ${live ? "on" : ""}`} /></h1>
        <p className="blurb">
          Agents write BAML against canary; each run becomes a trophy, findings dedupe into issues.
        </p>
        <p className="mute" style={{ fontSize: 13 }}>
          ${s.totals.costUsd.toFixed(2)} est · canary {readyBuild ? readyBuild.sha.slice(0, 8) : "—"} ·{" "}
          <button className="linkbtn" onClick={() => setLive((v) => !v)}>{live ? "live ⏸" : "paused ▶"}</button>
          {" "}· {s.generatedAt}
        </p>
      </header>

      {/* pipeline flow: in-flight → tasks → trophies → issues */}
      <div className="flow">
        <div className="flow-col">
          <div className="flow-cap">in flight</div>
          {inflightTasks.length === 0 ? (
            <div className="mini mini-mute">idle</div>
          ) : inflightTasks.slice(0, 4).map((f) => (
            <div key={f.id} className={`mini mini-${STAGE[f.stage] ?? "warn"}`}>
              <span className="dot" />{f.stage}
              <div className="mini-body">{f.label.slice(0, 36)}</div>
              <div className="mini-foot mono">{ago(f.sinceMs)}</div>
            </div>
          ))}
        </div>
        <span className="arrow">→</span>
        <DbNode name="tasks" table="tasks" breakdown={s.counts.tasks} />
        <span className="arrow">→</span>
        <DbNode name="trophies" table="trophies" breakdown={s.counts.trophies} />
        <span className="arrow">→</span>
        <DbNode name="issues" table="issues" breakdown={s.counts.issues} />
      </div>

      {/* recent runs table -> trophy details */}
      <h2>Recent runs</h2>
      {s.runs.length === 0 ? <p className="mute">no runs yet.</p> : (
        <table className="runtable">
          <thead>
            <tr>
              <th>outcome</th><th>task</th><th>src</th>
              <th className="r">turns</th><th className="r">api</th>
              <th className="r">cost</th><th className="r">issues</th><th className="r">age</th>
            </tr>
          </thead>
          <tbody>
            {s.runs.map((r) => (
              <tr key={r.trophyId} className="runrow">
                <td><span className={`statpill ${r.outcome === "success" ? "completed" : r.outcome === "failed" ? "failed" : "partial"}`}>{r.outcome}</span></td>
                <td><Link href={`/runs/${r.trophyId}`}>{r.prompt.slice(0, 70)}</Link></td>
                <td className="mono mute">{r.source}</td>
                <td className="r mono">{r.turns ?? "-"}</td>
                <td className="r mono">{r.apiCalls ?? "-"}</td>
                <td className="r mono">${(r.costUsd ?? 0).toFixed(2)}</td>
                <td className="r mono">{r.findings || ""}</td>
                <td className="r mono mute">{ago(now - r.createdAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
