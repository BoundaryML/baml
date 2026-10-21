// Graph: the declarations the session is about, as a dependency graph
// clustered by file, with the code being edited always in view (floating over
// the graph, or in a panel above it). When the agent edits, a card with the
// changed lines flies from the code into the node it changed; a new
// declaration is born where the card lands. Pointing at a line of code lights
// up its declaration. When the agent runs something, the functions that
// executed light up in call order. Layout changes glide rather than jump, so
// a node keeps its identity while the graph grows around it.

import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkNode } from 'elkjs/lib/elk-api';
import {
  type ComponentType,
  type PointerEvent,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { EditCaptionLine } from '../components/caption';
import { type DiffLine, EditDiff } from '../components/edit-diff';
import { RunCard } from '../components/run-card';
import { KindBadge, splitPath } from '../components/symbols';
import type { Edit } from '../engine/model';
import { useModel, usePlayer } from '../runtime';
import { Camera, fitView, type Rect } from './camera';
import { buildGraph, type GraphData, nodeId } from './graph-model';

const elk = new ELK();
const NODE_H = 28;
export const MIN_K = 0.1;
const MAX_K = 1.6;
const TWEEN_MS = 650;
/** Below this zoom node labels stop being readable. */
export const READABLE_K = 0.7;

interface Layout {
  nodes: Map<string, Rect>;
  clusters: Map<string, Rect>;
  bounds: Rect;
}

function nodeWidth(label: string): number {
  // Badge, gap, padding, and border take 41px; 12px monospace is ~7.25px a character.
  return Math.round(50 + label.length * 7.4);
}

async function layoutGraph(data: GraphData): Promise<Layout> {
  const root: ElkNode = {
    children: data.files.map((path) => ({
      children: data.nodes
        .filter((n) => n.path === path)
        .map((n) => ({ height: NODE_H, id: n.id, width: nodeWidth(n.label) })),
      id: `file:${path}`,
      layoutOptions: { 'elk.padding': '[top=36,left=14,bottom=14,right=14]' },
    })),
    edges: data.edges.map((e) => ({
      id: e.id,
      sources: [e.from],
      targets: [e.to],
    })),
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'RIGHT',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.layered.spacing.nodeNodeBetweenLayers': '56',
      'elk.padding': '[top=24,left=24,bottom=24,right=24]',
      'elk.spacing.componentComponent': '48',
      'elk.spacing.nodeNode': '12',
    },
  };
  const out = await elk.layout(root);
  const nodes = new Map<string, Rect>();
  const clusters = new Map<string, Rect>();
  for (const cluster of out.children ?? []) {
    const cx = cluster.x ?? 0;
    const cy = cluster.y ?? 0;
    clusters.set(cluster.id.slice(5), {
      h: cluster.height ?? 0,
      w: cluster.width ?? 0,
      x: cx,
      y: cy,
    });
    for (const n of cluster.children ?? []) {
      nodes.set(n.id, {
        h: n.height ?? NODE_H,
        w: n.width ?? 0,
        x: cx + (n.x ?? 0),
        y: cy + (n.y ?? 0),
      });
    }
  }
  return {
    bounds: { h: out.height ?? 0, w: out.width ?? 0, x: 0, y: 0 },
    clusters,
    nodes,
  };
}

function lerpRect(a: Rect, b: Rect, t: number): Rect {
  return {
    h: a.h + (b.h - a.h) * t,
    w: a.w + (b.w - a.w) * t,
    x: a.x + (b.x - a.x) * t,
    y: a.y + (b.y - a.y) * t,
  };
}

const ease = (t: number) => 1 - (1 - t) ** 3;

export interface CanvasProps {
  /** Node the reader is pointing at from the code (hovered or clicked line). */
  pointed: { id: string; pinned: boolean } | null;
  /** Screen pixels at the top of the canvas covered by the floating code card. */
  topInset: number;
  /** Where an edit's card starts its flight, in canvas coordinates. */
  flyerOrigin: { x: number; y: number };
}

/** The view that fits `rect` into the canvas below `topInset`. */
export function fitBelow(
  rect: Rect,
  camera: Camera,
  topInset: number,
  margin: number,
  maxK: number,
) {
  const area = { h: Math.max(80, camera.vh - topInset), w: camera.vw };
  const view = fitView(rect, area.w, area.h, margin, maxK, MIN_K);
  // Shift so the rect is centered in the uncovered area rather than the whole canvas.
  return { ...view, cy: view.cy - topInset / 2 / view.k };
}

function GraphCanvas({ pointed, topInset, flyerOrigin }: CanvasProps) {
  const model = useModel();
  const snap = usePlayer();
  const stageRef = useRef<HTMLDivElement>(null);
  const worldRef = useRef<HTMLDivElement>(null);
  const [following, setFollowing] = useState(true);
  const followingRef = useRef(following);
  followingRef.current = following;

  const data = useMemo(
    () => buildGraph(model, snap.played),
    [model.version, snap.played],
  );
  const signature = data.nodes.map((n) => n.id).join('|') + data.edges.length;

  // Target layout from ELK, and the displayed layout tweening toward it.
  const [shown, setShown] = useState<Layout | null>(null);
  const shownRef = useRef<Layout | null>(null);
  shownRef.current = shown;
  useEffect(() => {
    let cancelled = false;
    let frame = 0;
    void layoutGraph(data).then((target) => {
      if (cancelled) return;
      const from = shownRef.current;
      if (!from) {
        setShown(target);
        return;
      }
      const start = performance.now();
      const tick = () => {
        const t = ease(Math.min(1, (performance.now() - start) / TWEEN_MS));
        const mix = (fromMap: Map<string, Rect>, toMap: Map<string, Rect>) => {
          const m = new Map<string, Rect>();
          for (const [id, r] of toMap)
            m.set(id, fromMap.has(id) ? lerpRect(fromMap.get(id)!, r, t) : r);
          return m;
        };
        setShown({
          bounds: target.bounds,
          clusters: mix(from.clusters, target.clusters),
          nodes: mix(from.nodes, target.nodes),
        });
        if (t < 1) frame = requestAnimationFrame(tick);
      };
      frame = requestAnimationFrame(tick);
    });
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
    };
  }, [signature]);

  const camera = useMemo(
    () =>
      new Camera((view) => {
        const world = worldRef.current;
        const stage = stageRef.current;
        if (!world || !stage) return;
        world.style.transform = camera.transform(view);
        const size = 24 * view.k;
        stage.style.backgroundSize = `${size}px ${size}px`;
        stage.style.backgroundPosition = `${camera.vw / 2 - view.cx * view.k}px ${camera.vh / 2 - view.cy * view.k}px`;
      }),
    [],
  );
  useEffect(() => () => camera.stop(), [camera]);
  // The canvas size; camera fits wait for the first measurement.
  const [size, setSize] = useState({ h: 0, w: 0 });

  useLayoutEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      camera.vw = r.width;
      camera.vh = r.height;
      camera.set(camera.view);
      setSize({ h: r.height, w: r.width });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [camera]);

  // While following, show the whole graph if it fits at a readable size;
  // otherwise stay readable and center on where the agent last worked.
  const bounds = shown?.bounds;
  const boundsKey = bounds
    ? `${Math.round(bounds.w / 40)}x${Math.round(bounds.h / 40)}`
    : '';
  const focusId = useMemo(() => {
    let best: { id: string; lastEdit: number } | null = null;
    for (const n of data.nodes)
      if (!best || n.lastEdit > best.lastEdit)
        best = { id: n.id, lastEdit: n.lastEdit };
    return best?.id;
  }, [data]);
  const first = useRef(true);
  useEffect(() => {
    if (size.w === 0 || !shown || !bounds || !followingRef.current) return;
    let target = fitBelow(bounds, camera, topInset, 40, 1.1);
    const focus = focusId ? shown.nodes.get(focusId) : undefined;
    if (target.k < READABLE_K && focus) {
      target = fitBelow(focus, camera, topInset, 0, READABLE_K);
      target.k = READABLE_K;
    }
    if (first.current) {
      first.current = false;
      camera.set(target);
    } else {
      camera.flyTo(target, 700);
    }
  }, [boundsKey, focusId, shown !== null, topInset, size.w, size.h]);

  // A clicked line brings its node into view.
  useEffect(() => {
    if (!pointed?.pinned || !shown) return;
    const r = shown.nodes.get(pointed.id);
    if (!r) return;
    setFollowing(false);
    const view = fitBelow(
      r,
      camera,
      topInset,
      0,
      Math.max(camera.view.k, READABLE_K),
    );
    view.k = Math.max(camera.view.k, READABLE_K);
    camera.flyTo(view, 500);
  }, [pointed]);

  // Pan and zoom.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      setFollowing(false);
      if (e.ctrlKey || e.metaKey) {
        const r = el.getBoundingClientRect();
        camera.zoomAt(
          Math.exp(-e.deltaY * 0.01),
          e.clientX - r.left,
          e.clientY - r.top,
          MIN_K,
          MAX_K,
        );
      } else {
        camera.panBy(-e.deltaX, -e.deltaY);
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [camera]);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button, .aa-run'))
      return;
    drag.current = { x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent) => {
    if (!drag.current) return;
    const dx = e.clientX - drag.current.x;
    const dy = e.clientY - drag.current.y;
    if (Math.abs(dx) + Math.abs(dy) < 2) return;
    setFollowing(false);
    camera.panBy(dx, dy);
    drag.current = { x: e.clientX, y: e.clientY };
  };

  // What the active edit hits, and what the latest run executed.
  const active = snap.active;
  const hitIds = new Set(
    active
      ? active.edit.symbols.map((s) => nodeId(active.edit.path, s.key))
      : [],
  );
  const landAt = active ? active.travelMs + active.writeMs * 0.8 : 0;
  const lastRun =
    snap.runsShown > 0 ? model.runs[snap.runsShown - 1] : undefined;
  const shownRun =
    lastRun && lastRun.afterEdit >= snap.played && !active
      ? lastRun
      : undefined;
  const ranOrder = new Map<string, number>();
  shownRun?.frames?.forEach((f, i) => {
    const n = data.nodes.find(
      (node) => node.path === f.path && node.name === f.name,
    );
    if (n && !ranOrder.has(n.id)) ranOrder.set(n.id, i);
  });

  const nodesById = new Map(data.nodes.map((n) => [n.id, n]));
  const activeFile = active?.edit.path;

  return (
    <div
      className="aa-stage aa-graph"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => {
        drag.current = null;
      }}
      ref={stageRef}
    >
      {data.nodes.length === 0 && (
        <div className="aa-waiting">
          <h2>No declarations changed yet</h2>
          <p>
            Types and functions the agent touches appear here, linked to the
            declarations that use them.
          </p>
        </div>
      )}
      <div className="aa-stage-world" ref={worldRef}>
        {shown &&
          data.files.map((path) => {
            const r = shown.clusters.get(path);
            if (!r) return null;
            const { dir, name } = splitPath(path);
            return (
              <div
                className="aa-graph-cluster"
                data-active={path === activeFile}
                key={path}
                style={{ height: r.h, left: r.x, top: r.y, width: r.w }}
              >
                <div className="aa-graph-cluster-label" title={path}>
                  <b>{name}</b> {dir}
                </div>
              </div>
            );
          })}
        {shown && (
          <svg aria-hidden="true" className="aa-route" height={1} width={1}>
            {data.edges.map((e) => {
              const a = shown.nodes.get(e.from);
              const b = shown.nodes.get(e.to);
              if (!a || !b) return null;
              const hot = hitIds.has(e.from) || hitIds.has(e.to);
              return (
                <path
                  className="aa-graph-edge"
                  d={edgePath(a, b)}
                  data-hot={hot}
                  key={e.id}
                />
              );
            })}
          </svg>
        )}
        {shown &&
          data.nodes.map((n) => {
            const r = shown.nodes.get(n.id);
            if (!r) return null;
            const hit = hitIds.has(n.id);
            const born =
              hit && n.state === 'added' && n.lastEdit === snap.played - 1;
            const ran = ranOrder.get(n.id);
            return (
              <div
                className="aa-graph-node"
                data-born={born}
                data-hit={hit && !born}
                data-pointed={pointed?.id === n.id}
                data-ran={ran !== undefined ? shownRun?.status : undefined}
                data-state={n.state}
                key={`${n.id}${hit ? `:${active?.edit.index}` : ''}`}
                style={{
                  animationDelay: hit
                    ? `${landAt}ms`
                    : ran !== undefined
                      ? `${ran * 180}ms`
                      : undefined,
                  left: r.x,
                  top: r.y,
                  width: r.w,
                }}
                title={`${n.kind} ${n.label}\n${n.path}`}
              >
                <KindBadge kind={n.kind} />
                <span className="aa-graph-label">{n.label}</span>
              </div>
            );
          })}
      </div>
      {active && shown && (
        <Flyer
          camera={camera}
          edit={active.edit}
          key={active.edit.index}
          origin={flyerOrigin}
          target={targetFor(active.edit, hitIds, shown, nodesById)}
          travelMs={active.travelMs}
          writeMs={active.writeMs}
        />
      )}
      {shownRun && (
        <div className="aa-graph-run">
          <RunCard active={snap.activeRun?.run === shownRun} run={shownRun} />
        </div>
      )}
      {!following && (
        <button
          className="aa-follow"
          onClick={() => {
            setFollowing(true);
            if (bounds)
              camera.flyTo(fitBelow(bounds, camera, topInset, 40, 1.1), 600);
          }}
          type="button"
        >
          <b>Fit the graph</b>
        </button>
      )}
    </div>
  );
}

type GraphLayoutMode = 'overlay' | 'split';

function readMode(): GraphLayoutMode {
  try {
    return window.localStorage.getItem('agent-activity.graph-layout') ===
      'split'
      ? 'split'
      : 'overlay';
  } catch {
    return 'overlay';
  }
}

export function GraphView() {
  return <CodeGraphHost Canvas={GraphCanvas} />;
}

/**
 * A graph canvas with the code beside it: either floating over the top of the
 * graph, or in a panel above it. Hovering a line lights up the declaration it
 * belongs to; clicking one also brings that node into view.
 */
export function CodeGraphHost({
  Canvas,
}: {
  Canvas: ComponentType<CanvasProps>;
}) {
  const model = useModel();
  const snap = usePlayer();
  const [mode, setMode] = useState<GraphLayoutMode>(readMode);
  const [hovered, setHovered] = useState<string | null>(null);
  const [pinned, setPinned] = useState<string | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const [hostSize, setHostSize] = useState({ h: 600, w: 900 });
  useLayoutEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      setHostSize({ h: r.height, w: r.width });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const edit = snap.active?.edit ?? model.edits[snap.played - 1];
  useEffect(() => {
    setHovered(null);
  }, [edit]);

  const onLine = (line: DiffLine | null, action: 'hover' | 'click') => {
    const id = line && edit ? declarationAt(model, edit, line) : null;
    if (action === 'hover') setHovered(id);
    else setPinned((prev) => (prev === id ? null : id));
  };

  const changeMode = (next: GraphLayoutMode) => {
    setMode(next);
    try {
      window.localStorage.setItem('agent-activity.graph-layout', next);
    } catch {
      // Storage can be unavailable; the choice just won't persist.
    }
  };

  const pointedId = hovered ?? pinned;
  const pointed = pointedId
    ? { id: pointedId, pinned: pointedId === pinned && hovered === null }
    : null;
  const cardW = Math.min(760, hostSize.w - 32);
  const cardH = Math.round(Math.min(340, hostSize.h * 0.42));
  const overlayOrigin = {
    x: (hostSize.w - cardW) / 2 + 24,
    y: 12 + cardH - 60,
  };

  const code = edit ? (
    <section
      aria-label={edit.path}
      className="aa-pane aa-graph-code"
      data-active={snap.active?.edit === edit}
    >
      <header className="aa-pane-head">
        <span className="aa-pane-path" title={edit.path}>
          <span className="dir">{`‎${splitPath(edit.path).dir}‎`}</span>
          <span className="name">{splitPath(edit.path).name}</span>
        </span>
        <span className="aa-pane-meta">
          {snap.active?.edit === edit ? (
            <span className="aa-writing">writing</span>
          ) : null}
          <span>Point at a line to find its declaration</span>
        </span>
      </header>
      <EditCaptionLine edit={edit} />
      <div className="aa-pane-body">
        <EditDiff
          active={snap.active}
          context={mode === 'split' ? 8 : 4}
          edit={edit}
          onLine={onLine}
        />
      </div>
    </section>
  ) : null;

  return (
    <div className="aa-graph-host" data-mode={mode} ref={hostRef}>
      <fieldset
        aria-label="Graph layout"
        className="aa-graph-mode aa-segmented aa-segmented--small"
      >
        <button
          aria-pressed={mode === 'overlay'}
          onClick={() => changeMode('overlay')}
          type="button"
        >
          Code over graph
        </button>
        <button
          aria-pressed={mode === 'split'}
          onClick={() => changeMode('split')}
          type="button"
        >
          Code above graph
        </button>
      </fieldset>
      {mode === 'split' ? (
        <>
          <div className="aa-graph-split-code">{code}</div>
          <div className="aa-graph-split-canvas">
            <Canvas
              flyerOrigin={{ x: 24, y: -40 }}
              pointed={pointed}
              topInset={0}
            />
          </div>
        </>
      ) : (
        <>
          <Canvas
            flyerOrigin={overlayOrigin}
            pointed={pointed}
            topInset={edit ? cardH + 24 : 0}
          />
          {code && (
            <div
              className="aa-graph-float"
              style={{ maxHeight: cardH, width: cardW }}
            >
              {code}
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** The graph node for the declaration containing `line` of `edit`. */
function declarationAt(
  model: ReturnType<typeof useModel>,
  edit: Edit,
  line: DiffLine,
): string | null {
  const outline = model.outlineAt(
    edit.path,
    line.side === 'new' ? edit.index + 1 : edit.index,
  );
  let best: { key: string; span: number } | null = null;
  for (const sym of outline) {
    if (line.line < sym.start || line.line > sym.end) continue;
    const span = sym.end - sym.start;
    // The innermost declaration wins: a method over its impl or class.
    if (!best || span < best.span) best = { key: sym.key, span };
  }
  return best ? nodeId(edit.path, best.key) : null;
}

function edgePath(a: Rect, b: Rect): string {
  const forward = b.x >= a.x + a.w;
  const x1 = forward ? a.x + a.w : a.x;
  const y1 = a.y + a.h / 2;
  const x2 = forward ? b.x : b.x + b.w;
  const y2 = b.y + b.h / 2;
  const dx = Math.max(30, Math.abs(x2 - x1) * 0.5) * (forward ? 1 : -1);
  return `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`;
}

function targetFor(
  edit: Edit,
  hitIds: Set<string>,
  layout: Layout,
  nodes: Map<string, { path: string }>,
): Rect | null {
  for (const id of hitIds) {
    const r = layout.nodes.get(id);
    if (r && nodes.has(id)) return r;
  }
  return layout.clusters.get(edit.path) ?? null;
}

/**
 * A card with the edit's changed lines. It appears in the corner during the
 * travel phase, then flies into the node it changed and shrinks away.
 */
export function Flyer({
  edit,
  travelMs,
  writeMs,
  target,
  camera,
  origin,
}: {
  edit: Edit;
  travelMs: number;
  writeMs: number;
  target: Rect | null;
  camera: Camera;
  origin: { x: number; y: number };
}) {
  const [phase, setPhase] = useState<'in' | 'fly' | 'gone'>('in');
  useEffect(() => {
    const a = setTimeout(() => setPhase('fly'), travelMs);
    const b = setTimeout(() => setPhase('gone'), travelMs + writeMs * 0.85);
    return () => {
      clearTimeout(a);
      clearTimeout(b);
    };
  }, []);
  const lines = edit.ops.filter((op) => op.type !== 'equal').slice(0, 7);
  const start = origin;
  let transform = `translate(${start.x}px, ${start.y}px)`;
  let opacity = 1;
  if (phase !== 'in' && target) {
    const { cx, cy, k } = camera.view;
    const sx = camera.vw / 2 + (target.x + target.w / 2 - cx) * k;
    const sy = camera.vh / 2 + (target.y + target.h / 2 - cy) * k;
    transform = `translate(${sx - 20}px, ${sy - 10}px) scale(0.14)`;
    opacity = phase === 'gone' ? 0 : 0.85;
  } else if (phase !== 'in') {
    opacity = 0;
  }
  return (
    <div
      className="aa-flyer"
      data-phase={phase}
      style={{
        opacity,
        transform,
        transition:
          phase === 'in'
            ? 'opacity 200ms'
            : `transform ${writeMs * 0.8}ms cubic-bezier(0.5, 0, 0.3, 1), opacity ${writeMs * 0.8}ms ease-in`,
      }}
    >
      <div className="aa-flyer-head">
        <b>{splitPath(edit.path).name}</b>
        <span>
          <span className="add">+{edit.added}</span>{' '}
          <span className="del">−{edit.removed}</span>
        </span>
      </div>
      <pre>
        {lines.map((op) => (
          <div
            className={op.type === 'insert' ? 'add' : 'del'}
            key={op.type === 'delete' ? `-${op.oldIndex}` : `+${op.newIndex}`}
          >
            {op.type === 'insert' ? '+ ' : '− '}
            {op.text}
          </div>
        ))}
      </pre>
    </div>
  );
}
