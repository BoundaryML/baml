'use client';

import cytoscape from 'cytoscape';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';

import { InlineCode } from '@/components/ui/inline-code';
import { Pulse } from '@/components/ui/pulse';
import { cn } from '@/lib/utils';

import type { LiveState } from './lib/data';
import { ago } from './lib/format';
import { BottomTabs, usePolledState } from './live-dashboard';
import NodePanel, { nodeHasPanel } from './node-panel';

const cnt = (o: Record<string, number>) =>
  Object.values(o).reduce((a, b) => a + b, 0);
const dbHref = (table: 'tasks' | 'trophies' | 'issues') =>
  `/db/${table}`;
// How far inside the container edge a node's center must be to count as "in the
// graph"; below this it's treated as dragged out and bounces back.
const IN_BOUNDS_INSET = 12;
const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, value));
const SPRING_TENSION = 240;
// The elastic snap-back scales with how far the node was flung from home (model
// px). cytoscape supports spring(tension, friction) at runtime but its types
// only list named easings, hence the cast.
//   friction: lower = less damping = more visible oscillations.
//   duration: longer settle for farther throws so the bounces play out.
const snapBack = (dist: number) => {
  const friction = clamp(14 - dist / 40, 3.5, 14);
  const duration = clamp(850 + dist * 3, 850, 2600);
  const easing = `spring(${SPRING_TENSION}, ${friction.toFixed(
    1,
  )})` as unknown as cytoscape.Css.TransitionTimingFunction;
  return { duration, easing };
};

// inflight stage -> the graph element id it belongs to
const STAGE_EL: Record<string, string> = {
  'baml-build': 'canary',
  dedup: 'e_trophies_issues',
  'notion-sync': 'e_issues_notion',
  worker: 'e_tasks_trophies',
};

/**
 * Builds the Cytoscape node/edge definitions for the pipeline graph from a snapshot.
 * Includes external (triggers, canary, notion) and db (tasks/trophies/issues) nodes
 * with live counts, plus the edges between them.
 * @param s - the current live state supplying counts and the ready canary build
 * @returns the array of Cytoscape element definitions
 */
function elements(s: LiveState): cytoscape.ElementDefinition[] {
  const baml = (s.builds ?? []).find((b) => b.status === 'ready');
  const N = (name: string, sub: string) => `${name}\n${sub}`;
  // Per-stage issue counts (issues are tallied by issueStatusLabel, so a
  // dispatched issue counts as "cursor"; redraft spans redraft + redrafting).
  const ic = s.counts.issues ?? {};
  const approvedN = ic['approved'] ?? 0;
  const toCursorN = (ic['cursor'] ?? 0) + (ic['fixing'] ?? 0);
  const redraftN = (ic['redraft'] ?? 0) + (ic['redrafting'] ?? 0);
  return [
    {
      data: {
        id: 'triggers',
        kind: 'ext',
        label: N('triggers', 'slack · cron'),
      },
      position: { x: 70, y: 90 },
    },
    {
      data: {
        id: 'canary',
        kind: 'ext',
        label: N(
          'baml alpha',
          baml
            ? (baml.ref ?? '').replace('baml-language-', '') ||
                baml.sha.slice(0, 8)
            : 'none',
        ),
      },
      position: { x: 70, y: 280 },
    },
    {
      data: {
        href: dbHref('tasks'),
        id: 'tasks',
        kind: 'db',
        label: N('tasks', String(cnt(s.counts.tasks))),
      },
      position: { x: 300, y: 185 },
    },
    {
      data: {
        href: dbHref('trophies'),
        id: 'trophies',
        kind: 'db',
        label: N('trophies', String(cnt(s.counts.trophies))),
      },
      position: { x: 560, y: 380 },
    },
    {
      data: {
        href: dbHref('issues'),
        id: 'issues',
        kind: 'db',
        label: N('issues', String(cnt(s.counts.issues))),
      },
      position: { x: 820, y: 185 },
    },
    {
      data: { id: 'notion', kind: 'ext', label: N('notion', 'board') },
      position: { x: 1060, y: 185 },
    },
    {
      data: {
        href: dbHref('issues'),
        id: 'approved',
        kind: 'db',
        label: N('approved', String(approvedN)),
      },
      position: { x: 1300, y: 70 },
    },
    {
      data: {
        href: dbHref('issues'),
        id: 'tocursor',
        kind: 'db',
        label: N('to cursor', String(toCursorN)),
      },
      position: { x: 1300, y: 200 },
    },
    {
      data: {
        href: dbHref('issues'),
        id: 'redraft',
        kind: 'db',
        label: N('redraft', String(redraftN)),
      },
      position: { x: 1060, y: 360 },
    },
    { data: { id: 'e_trig', label: '', source: 'triggers', target: 'tasks' } },
    {
      data: {
        id: 'e_canary',
        label: 'baml',
        source: 'canary',
        target: 'tasks',
      },
    },
    {
      data: {
        id: 'e_tasks_trophies',
        label: 'worker',
        source: 'tasks',
        target: 'trophies',
      },
    },
    {
      data: {
        id: 'e_trophies_issues',
        label: 'dedup',
        source: 'trophies',
        target: 'issues',
      },
    },
    {
      data: {
        id: 'e_issues_notion',
        label: 'notion-sync',
        source: 'issues',
        target: 'notion',
      },
    },
    {
      data: {
        id: 'e_notion_approved',
        label: 'approve',
        source: 'notion',
        target: 'approved',
      },
    },
    {
      data: {
        id: 'e_approved_cursor',
        label: 'fix-dispatch',
        source: 'approved',
        target: 'tocursor',
      },
    },
    {
      data: {
        id: 'e_notion_redraft',
        label: 'redraft',
        source: 'notion',
        target: 'redraft',
      },
    },
    {
      data: {
        id: 'e_redraft_issues',
        label: 'rewrite',
        source: 'redraft',
        target: 'issues',
      },
    },
  ];
}

const STYLE: cytoscape.StylesheetStyle[] = [
  {
    selector: 'node',
    style: {
      'background-color': '#ffffff',
      'border-color': '#dcd8d0',
      'border-width': 1,
      color: '#1a1a1a',
      'font-family': 'Iowan Old Style, Charter, Georgia, serif',
      'font-size': 13,
      height: 56,
      label: 'data(label)',
      'line-height': 1.25,
      shape: 'round-rectangle',
      'text-halign': 'center',
      'text-max-width': '118px',
      'text-valign': 'center',
      'text-wrap': 'wrap',
      width: 140,
    },
  },
  {
    selector: 'node[kind = "db"]',
    style: { 'background-color': '#fbfaf7', 'border-color': '#bcb6aa' },
  },
  {
    selector: 'node[kind = "ext"]',
    style: { 'background-color': '#f3f1ec', color: '#8a8a86' },
  },
  {
    selector: 'edge',
    style: {
      'arrow-scale': 1.1,
      color: '#8a8a86',
      'curve-style': 'bezier',
      'font-family': 'ui-monospace, Menlo, monospace',
      'font-size': 10,
      label: 'data(label)',
      'line-color': '#cfcabf',
      'target-arrow-color': '#bcb6aa',
      'target-arrow-shape': 'triangle',
      'text-background-color': '#faf8f3',
      'text-background-opacity': 1,
      'text-background-padding': '2px',
      width: 1.4,
    },
  },
  {
    selector: '.active',
    style: {
      'border-color': '#4a7a4a',
      'border-width': 2.5,
      color: '#4a7a4a',
      'line-color': '#4a7a4a',
      'target-arrow-color': '#4a7a4a',
      width: 3,
    },
  },
  {
    // Intro highlight: lights up one node on first load with a warm halo.
    selector: '.hintnode',
    style: {
      'background-color': '#fff8ee',
      'border-color': '#c98a3a',
      'border-width': 3,
      'overlay-color': '#e0a44a',
      'overlay-opacity': 0.18,
      'overlay-padding': 8,
    },
  },
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
  const [pop, setPop] = useState<{ id: string; x: number; y: number } | null>(
    null,
  );
  const [fullscreen, setFullscreen] = useState(false);
  // Which db node's data panel is open (fullscreen only); null when closed.
  const [panel, setPanel] = useState<string | null>(null);
  // Intro hint anchored at a node's rendered position; null once dismissed.
  const [hint, setHint] = useState<{ x: number; y: number } | null>(null);
  const hintTimer = useRef<number | null>(null);
  // Let the (once-bound) cytoscape tap handlers read current popover/fullscreen.
  const popRef = useRef(pop);
  popRef.current = pop;
  const fullscreenRef = useRef(fullscreen);
  fullscreenRef.current = fullscreen;
  const panelRef = useRef(panel);
  panelRef.current = panel;

  const recenter = () => {
    const cy = cyRef.current;
    if (!cy) return;
    cy.animate(
      { fit: { eles: cy.elements(), padding: 36 } },
      { duration: 350, easing: 'ease-in-out-cubic' },
    );
  };

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    let cy: cytoscape.Core | null = null;

    const start = () => {
      if (cy || !el.clientWidth || !el.clientHeight) return; // wait until sized
      cy = cytoscape({
        autoungrabify: false,
        boxSelectionEnabled: false,
        container: el,
        elements: elements(initial),
        layout: { name: 'preset' },
        style: STYLE,
        userZoomingEnabled: false,
      });
      cy.fit(undefined, 36);
      // First-load hint: light up a node and point a callout at it.
      const dismissHint = () => {
        cyRef.current?.$('.hintnode').removeClass('hintnode');
        if (hintTimer.current) {
          clearTimeout(hintTimer.current);
          hintTimer.current = null;
        }
        setHint(null);
      };
      const hintNode = cy.getElementById('tasks');
      if (hintNode.nonempty()) {
        hintNode.addClass('hintnode');
        const rp = hintNode.renderedPosition();
        setHint({ x: rp.x, y: rp.y });
        hintTimer.current = window.setTimeout(dismissHint, 3000);
        // Dismiss the moment the user touches the graph — pointer-down (tapstart),
        // dragging the background (pan), or scroll/pinch (zoom) — not only on a
        // completed tap.
        cy.one('tapstart pan zoom', dismissHint);
      }
      cy.on('tap', 'node[kind = "db"]', (e) => {
        const node = e.target as cytoscape.NodeSingular;
        // In fullscreen, show the node's data in the side panel instead of
        // navigating away to the full /db view.
        if (fullscreenRef.current && nodeHasPanel(node.id())) {
          setPanel(node.id());
          return;
        }
        const href = node.data('href');
        if (href) router.push(href);
      });
      // Remembers, per node, its last resting spot: where it was last dropped
      // inside the graph (seeded with its layout position). Drag a node out and
      // release and it springs back here; drop it inside and that becomes the
      // new resting spot.
      const homePos = new Map<string, cytoscape.Position>();
      const centerInBounds = (node: cytoscape.NodeSingular) => {
        const rp = node.renderedPosition();
        return (
          rp.x >= IN_BOUNDS_INSET &&
          rp.x <= el.clientWidth - IN_BOUNDS_INSET &&
          rp.y >= IN_BOUNDS_INSET &&
          rp.y <= el.clientHeight - IN_BOUNDS_INSET
        );
      };
      cy.on('grab', 'node', (e) => {
        el.style.cursor = 'grabbing';
        const node = e.target as cytoscape.NodeSingular;
        if (!homePos.has(node.id()))
          homePos.set(node.id(), { ...node.position() });
      });
      cy.on('free', 'node', () => {
        el.style.cursor = 'grab';
      });
      cy.on('mouseover', 'node[kind = "db"], edge, node[kind = "ext"]', () => {
        el.style.cursor = 'pointer';
      });
      cy.on('mouseout', 'node[kind = "db"], edge, node[kind = "ext"]', () => {
        el.style.cursor = 'grab';
      });
      cy.on('dragfree', 'node', (e) => {
        const node = e.target as cytoscape.NodeSingular;
        if (centerInBounds(node)) {
          // Dropped inside: this is the new resting spot.
          homePos.set(node.id(), { ...node.position() });
          return;
        }
        const home = homePos.get(node.id());
        if (!home) return;
        const pos = node.position();
        const dist = Math.hypot(pos.x - home.x, pos.y - home.y);
        const { duration, easing } = snapBack(dist);
        node.stop();
        node.animate({ position: { ...home } }, { duration, easing });
      });
      const popup = (e: cytoscape.EventObject) => {
        const rp = e.renderedPosition || e.target.renderedPosition();
        setPop({ id: e.target.id(), x: rp.x, y: rp.y });
      };
      cy.on('tap', 'edge', popup);
      cy.on('tap', 'node[kind = "ext"]', popup);
      cy.on('tap', (e) => {
        if (e.target !== cy) return;
        // Empty-space tap: dismiss an open popover, else toggle fullscreen.
        if (popRef.current) setPop(null);
        else setFullscreen((v) => !v);
      });
      cyRef.current = cy;
    };

    start();
    // If the container wasn't sized yet, init as soon as it is; afterwards keep
    // the renderer in sync with container size.
    const ro = new ResizeObserver(() => {
      if (!cy) start();
      else cy.resize();
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (hintTimer.current) clearTimeout(hintTimer.current);
      cy?.destroy();
      cyRef.current = null;
    };
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
          if (n.nonempty()) n.data('label', el.data.label);
        }
      }
      cy.elements().removeClass('active');
      for (const f of s.inflight) {
        const id = STAGE_EL[f.stage];
        if (id) cy.getElementById(id).addClass('active');
      }
    });
  }, [s]);

  // On fullscreen toggle: resize the renderer to the new container, refit, and
  // (when entering) lock body scroll + allow Esc to exit. Exiting closes any
  // open data panel.
  useEffect(() => {
    const cy = cyRef.current;
    setPop(null);
    if (!fullscreen) setPanel(null);
    const id = requestAnimationFrame(() => {
      cy?.resize();
      cy?.animate(
        { fit: { eles: cy.elements(), padding: 36 } },
        { duration: 300, easing: 'ease-in-out-cubic' },
      );
    });
    if (!fullscreen) return () => cancelAnimationFrame(id);
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== 'Escape') return;
      // Esc closes the panel first, then exits fullscreen.
      if (panelRef.current) setPanel(null);
      else setFullscreen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => {
      cancelAnimationFrame(id);
      document.body.style.overflow = prevOverflow;
      window.removeEventListener('keydown', onKey);
    };
  }, [fullscreen]);

  // Reflow the graph into the space left by the data panel opening/closing.
  useEffect(() => {
    if (!fullscreen) return;
    const cy = cyRef.current;
    const id = requestAnimationFrame(() => {
      cy?.resize();
      cy?.animate(
        { fit: { eles: cy.elements(), padding: 36 } },
        { duration: 260, easing: 'ease-in-out-cubic' },
      );
    });
    return () => cancelAnimationFrame(id);
  }, [panel, fullscreen]);

  const popItems = pop
    ? s.inflight.filter((f) => STAGE_EL[f.stage] === pop.id)
    : [];

  // shared chrome-button style for the recenter / exit-fullscreen controls
  const chromeBtn =
    'absolute z-[6] inline-flex items-center justify-center p-0 cursor-pointer ' +
    'text-[#6b665c] bg-background border border-border rounded-md ' +
    'shadow-[0_1px_2px_rgba(0,0,0,0.06)] transition-colors duration-[120ms] ' +
    'hover:text-[#1a1a1a] hover:border-[#bcb6aa]';

  return (
    <div>
      <header className="mb-9 max-[640px]:mb-6">
        <h1 className="mb-1.5 text-[28px] font-medium tracking-[-0.01em] max-[640px]:text-[22px]">
          agent tries baml <Pulse on={live} />
        </h1>
        <p className="text-[15px] leading-[1.55] text-muted-foreground">
          A live look at an autonomous agent that uses BAML on real tasks. It
          picks up work, runs it, and files what it finds. Each node is a stage
          in that pipeline. Click a node to open its data, and watch edges glow
          green where work is in flight.
        </p>
        <p className="text-[13px] text-muted-foreground">
          ${s.totals.costUsd.toFixed(2)} est ·{' '}
          <button
            className="cursor-pointer border-0 bg-transparent p-0 text-link"
            onClick={() => setLive((v) => !v)}
          >
            {live ? 'live ⏸' : 'paused ▶'}
          </button>{' '}
          · {s.generatedAt}
        </p>
        <p className="mono mt-2 flex flex-wrap gap-4 text-xs text-muted-foreground">
          <span>
            <b className="font-semibold text-foreground">
              {s.agents.activeTasks}
            </b>{' '}
            active tasks
          </span>
          <span>
            <b className="font-semibold text-foreground">{s.agents.workers}</b>{' '}
            workers
          </span>
          <span>
            <b className="font-semibold text-foreground">{s.agents.dedupers}</b>{' '}
            dedupers
          </span>
          <span>
            <b className="font-semibold text-foreground">{s.agents.fixers}</b>{' '}
            fixers
          </span>
        </p>
      </header>

      <div
        className={cn(
          'relative my-2 mb-[30px]',
          fullscreen && 'fixed inset-0 z-[60] m-0 bg-background p-5',
          panel && fullscreen && 'pr-[412px]',
        )}
      >
        <div
          className={cn('graph', fullscreen && 'h-full rounded-md')}
          ref={box}
        />
        {fullscreen && panel ? (
          <NodePanel nodeId={panel} onClose={() => setPanel(null)} s={s} />
        ) : null}
        {hint && !fullscreen ? (
          <div className="ghint" style={{ left: hint.x, top: hint.y }}>
            click to see node data
          </div>
        ) : null}
        <button
          aria-label="Recenter graph"
          className={cn(
            chromeBtn,
            'size-[30px]',
            fullscreen
              ? panel
                ? 'right-[408px] bottom-7'
                : 'right-7 bottom-7'
              : 'right-3 bottom-3',
          )}
          onClick={recenter}
          title="Recenter"
          type="button"
        >
          <svg
            aria-hidden="true"
            fill="none"
            height="15"
            stroke="currentColor"
            strokeLinecap="round"
            strokeWidth="2"
            viewBox="0 0 24 24"
            width="15"
          >
            <circle cx="12" cy="12" r="3.5" />
            <line x1="12" x2="12" y1="2" y2="6" />
            <line x1="12" x2="12" y1="18" y2="22" />
            <line x1="2" x2="6" y1="12" y2="12" />
            <line x1="18" x2="22" y1="12" y2="12" />
          </svg>
        </button>
        {fullscreen ? (
          <button
            aria-label="Exit fullscreen"
            className={cn(
              chromeBtn,
              'size-8 text-xl leading-none',
              panel ? 'top-7 right-[408px]' : 'top-7 right-7',
            )}
            onClick={() => setFullscreen(false)}
            title="Exit fullscreen"
            type="button"
          >
            ×
          </button>
        ) : null}
        {pop ? (
          <div
            className="absolute z-[5] min-w-[180px] max-w-[340px] -translate-x-1/2 translate-y-2.5 rounded border border-foreground bg-background px-2.5 py-2 text-[12.5px] shadow-[0_4px_14px_rgba(0,0,0,0.1)]"
            style={{ left: pop.x, top: pop.y }}
          >
            <div className="mb-1.5 flex items-center justify-between text-[11px] uppercase tracking-[0.06em] text-muted-foreground">
              in flight{' '}
              <button
                className="cursor-pointer border-0 bg-transparent p-0 text-[15px] leading-none text-muted-foreground"
                onClick={() => setPop(null)}
              >
                ×
              </button>
            </div>
            {popItems.length === 0 ? (
              <div className="border-t border-border py-[3px] text-muted-foreground first:border-t-0">
                nothing in flight here
              </div>
            ) : (
              popItems.map((f) => (
                <div
                  className="border-t border-border py-[3px] first:border-t-0 [&>div]:my-px"
                  key={f.id}
                >
                  <div>
                    <b>{f.stage}</b>{' '}
                    <span className="mono text-muted-foreground">
                      {(f.claimedBy ?? 'agent').slice(0, 22)}
                    </span>
                  </div>
                  <div className="my-0.5 whitespace-normal [overflow-wrap:anywhere]">
                    <InlineCode text={f.label} />
                  </div>
                  <div className="mono text-[11px] text-muted-foreground">
                    call {f.id.slice(0, 8)} · {ago(f.sinceMs)}
                  </div>
                </div>
              ))
            )}
          </div>
        ) : null}
      </div>

      <BottomTabs s={s} now={now} />
    </div>
  );
}
