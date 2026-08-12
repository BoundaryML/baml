"use client";

import cytoscape from "cytoscape";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";

import type { LiveState } from "./lib/data";
import { ago } from "./lib/format";
import { usePolledState } from "./live-dashboard";

const cnt = (o: Record<string, number>) => Object.values(o).reduce((a, b) => a + b, 0);

// inflight stage -> the graph element id it belongs to
const STAGE_EL: Record<string, string> = {
  worker: "e_tasks_trophies",
  dedup: "e_trophies_issues", "notion-sync": "e_issues_notion", "baml-build": "canary",
};

/**
 * Builds the Cytoscape node/edge definitions for the pipeline graph from a snapshot.
 * Includes external (triggers, canary, notion) and db (tasks/trophies/issues) nodes
 * with live counts, plus the edges between them.
 * @param s - the current live state supplying counts and the ready canary build
 * @returns the array of Cytoscape element definitions
 */
function elements(s: LiveState): cytoscape.ElementDefinition[] {
  const baml = (s.builds ?? []).find((b) => b.status === "ready");
  const N = (name: string, sub: string) => `${name}\n${sub}`;
  return [
    { data: { id: "triggers", label: N("triggers", "slack · cron"), kind: "ext" }, position: { x: 70, y: 70 } },
    { data: { id: "canary", label: N("baml alpha", baml ? (baml.ref ?? "").replace("baml-language-", "") || baml.sha.slice(0, 8) : "none"), kind: "ext" }, position: { x: 70, y: 250 } },
    { data: { id: "tasks", label: N("tasks", String(cnt(s.counts.tasks))), kind: "db", href: "/db/tasks" }, position: { x: 300, y: 150 } },
    { data: { id: "trophies", label: N("trophies", String(cnt(s.counts.trophies))), kind: "db", href: "/db/trophies" }, position: { x: 570, y: 370 } },
    { data: { id: "issues", label: N("issues", String(cnt(s.counts.issues))), kind: "db", href: "/db/issues" }, position: { x: 840, y: 150 } },
    { data: { id: "notion", label: N("notion", "+ cursor"), kind: "ext" }, position: { x: 1060, y: 150 } },
    { data: { id: "e_trig", source: "triggers", target: "tasks", label: "" } },
    { data: { id: "e_canary", source: "canary", target: "tasks", label: "baml" } },
    { data: { id: "e_tasks_trophies", source: "tasks", target: "trophies", label: "worker" } },
    { data: { id: "e_trophies_issues", source: "trophies", target: "issues", label: "dedup" } },
    { data: { id: "e_issues_notion", source: "issues", target: "notion", label: "notion-sync" } },
  ];
}

const STYLE: cytoscape.StylesheetStyle[] = [
  {
    selector: "node",
    style: {
      shape: "round-rectangle", "background-color": "#ffffff",
      "border-width": 1, "border-color": "#dcd8d0",
      label: "data(label)", "text-wrap": "wrap", "text-max-width": "118px",
      "text-valign": "center", "text-halign": "center", "line-height": 1.25,
      "font-family": "Iowan Old Style, Charter, Georgia, serif", "font-size": 13,
      color: "#1a1a1a", width: 140, height: 56,
    },
  },
  { selector: 'node[kind = "db"]', style: { "border-color": "#bcb6aa", "background-color": "#fbfaf7" } },
  { selector: 'node[kind = "ext"]', style: { "background-color": "#f3f1ec", color: "#8a8a86" } },
  {
    selector: "edge",
    style: {
      width: 1.4, "line-color": "#cfcabf", "curve-style": "bezier",
      "target-arrow-shape": "triangle", "target-arrow-color": "#bcb6aa", "arrow-scale": 1.1,
      label: "data(label)", "font-size": 10, color: "#8a8a86",
      "font-family": "ui-monospace, Menlo, monospace",
      "text-background-color": "#faf8f3", "text-background-opacity": 1, "text-background-padding": "2px",
    },
  },
  { selector: ".active", style: { "border-color": "#4a7a4a", "border-width": 2.5, "line-color": "#4a7a4a", "target-arrow-color": "#4a7a4a", width: 3, color: "#4a7a4a" } },
];

/**
 * Client component rendering the interactive Cytoscape pipeline graph. Nodes link
 * to their db tables, edges glow green where work is in flight, and tapping an edge
 * or external node shows a popover of in-flight items. Also renders the recent-runs table.
 * @param initial - the server-rendered LiveState used to seed live polling
 * @returns the graph view
 */
export default function GraphView({ initial }: { initial: LiveState }) {
  const box = useRef<HTMLDivElement>(null);
  const cyRef = useRef<cytoscape.Core | null>(null);
  const router = useRouter();
  const { s, live, setLive } = usePolledState(initial);
  const sRef = useRef(s);
  sRef.current = s;
  const now = Date.now();
  const [pop, setPop] = useState<{ id: string; x: number; y: number } | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    let cy: cytoscape.Core | null = null;

    const start = () => {
      if (cy || !el.clientWidth || !el.clientHeight) return; // wait until sized
      cy = cytoscape({
        container: el,
        elements: elements(initial),
        style: STYLE,
        layout: { name: "preset" },
        userZoomingEnabled: false,
        boxSelectionEnabled: false,
        autoungrabify: false,
      });
      cy.fit(undefined, 36);
      cy.on("tap", "node[href]", (e) => {
        const href = e.target.data("href");
        if (href) router.push(href);
      });
      const popup = (e: cytoscape.EventObject) => {
        const rp = e.renderedPosition || e.target.renderedPosition();
        setPop({ id: e.target.id(), x: rp.x, y: rp.y });
      };
      cy.on("tap", "edge", popup);
      cy.on("tap", 'node[kind = "ext"]', popup);
      cy.on("tap", (e) => { if (e.target === cy) setPop(null); });
      cyRef.current = cy;
    };

    start();
    // If the container wasn't sized yet, init as soon as it is; afterwards keep
    // the renderer in sync with container size.
    const ro = new ResizeObserver(() => { if (!cy) start(); else cy.resize(); });
    ro.observe(el);
    return () => { ro.disconnect(); cy?.destroy(); cyRef.current = null; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // live: refresh labels + glow active edges (preserves dragged positions)
  useEffect(() => {
    const cy = cyRef.current;
    if (!cy) return;
    cy.batch(() => {
      for (const el of elements(s)) {
        if (!el.data.source) {
          const n = cy.getElementById(el.data.id as string);
          if (n.nonempty()) n.data("label", el.data.label);
        }
      }
      cy.elements().removeClass("active");
      for (const f of s.inflight) {
        const id = STAGE_EL[f.stage];
        if (id) cy.getElementById(id).addClass("active");
      }
    });
  }, [s]);

  const popItems = pop ? s.inflight.filter((f) => STAGE_EL[f.stage] === pop.id) : [];

  return (
    <div>
      <header className="page">
        <h1>baml-bench <span className={`pulse ${live ? "on" : ""}`} /></h1>
        <p className="blurb">Click a node to open it. Edges glow green where work is in flight.</p>
        <p className="mute" style={{ fontSize: 13 }}>
          ${s.totals.costUsd.toFixed(2)} est ·{" "}
          <button className="linkbtn" onClick={() => setLive((v) => !v)}>{live ? "live ⏸" : "paused ▶"}</button>
          {" "}· {s.generatedAt}
        </p>
        <p className="agents mono">
          <span><b>{s.agents.activeTasks}</b> active tasks</span>
          <span><b>{s.agents.workers}</b> workers</span>
          <span><b>{s.agents.dedupers}</b> dedupers</span>
          <span><b>{s.agents.fixers}</b> fixers</span>
        </p>
      </header>

      <div className="graphwrap">
        <div ref={box} className="graph" />
        {pop ? (
          <div className="gpop" style={{ left: pop.x, top: pop.y }}>
            <div className="gpop-head">
              in flight <button className="gpop-x" onClick={() => setPop(null)}>×</button>
            </div>
            {popItems.length === 0 ? (
              <div className="gpop-item mute">nothing in flight here</div>
            ) : popItems.map((f) => (
              <div key={f.id} className="gpop-item">
                <div><b>{f.stage}</b> <span className="mono mute">{(f.claimedBy ?? "agent").slice(0, 22)}</span></div>
                <div className="gpop-task">{f.label}</div>
                <div className="mono mute" style={{ fontSize: 11 }}>call {f.id.slice(0, 8)} · {ago(f.sinceMs)}</div>
              </div>
            ))}
          </div>
        ) : null}
      </div>

      <h2>Recent runs</h2>
      {s.runs.length === 0 ? <p className="mute">no runs yet.</p> : (
        <table className="runtable">
          <thead>
            <tr><th>outcome</th><th>task</th><th>src</th><th className="r">turns</th>
              <th className="r">api</th><th className="r">cost</th><th className="r">issues</th><th className="r">age</th></tr>
          </thead>
          <tbody>
            {s.runs.map((r) => (
              <tr key={r.trophyId} className="runrow">
                <td><span className={`statpill ${r.outcome === "success" ? "completed" : r.outcome === "failed" ? "failed" : "partial"}`}>{r.outcome}</span></td>
                <td><Link href={`/runs/${r.trophyId}`}>{r.prompt}</Link></td>
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
