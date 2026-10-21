// Stage: every touched file as a card on a canvas, grouped into one frame per
// directory, like artboards in a design tool. The camera follows the agent:
// each edit flies to its file along a zoom-out-pan-zoom-in path, so a jump
// across the project shows where it went, and a jump to a neighbor barely
// zooms at all. A trail links the last few files the agent visited. While the
// agent is quiet, the camera pulls back to show the whole map.

import {
  memo,
  type PointerEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { Caption, EditCaptionLine, useCaption } from '../components/caption';
import { EditDiff } from '../components/edit-diff';
import { RunCard } from '../components/run-card';
import { Counts, splitPath } from '../components/symbols';
import type { Edit } from '../engine/model';
import type { ActiveEdit } from '../engine/player';
import { useModel, usePlayer, useRuntime } from '../runtime';
import { Camera, fitView, type Rect, unionRects, type View } from './camera';
import { CARD_H, CARD_W, layoutStage } from './stage-layout';

const RUN_W = 400;
const OVERVIEW_AFTER_MS = 6000;
const MIN_K = 0.08;
const MAX_K = 2;
const TRAIL_LENGTH = 7;

export function StageView() {
  const model = useModel();
  const snap = usePlayer();
  const { focusRequest } = useRuntime();
  const stageRef = useRef<HTMLDivElement>(null);
  const worldRef = useRef<HTMLDivElement>(null);
  const [following, setFollowing] = useState(true);
  const [zoom, setZoom] = useState(1);
  const followingRef = useRef(following);
  followingRef.current = following;

  const paths = useMemo(
    () => model.touchedFiles(snap.played).map((f) => f.path),
    [model.version, snap.played],
  );
  const layout = useMemo(() => layoutStage(paths), [paths.join('\n')]);
  const layoutRef = useRef(layout);
  layoutRef.current = layout;

  const camera = useMemo(
    () =>
      new Camera((view) => {
        const world = worldRef.current;
        const stage = stageRef.current;
        if (!world || !stage) return;
        world.style.transform = camera.transform(view);
        const size = 24 * view.k;
        const ox = camera.vw / 2 - view.cx * view.k;
        const oy = camera.vh / 2 - view.cy * view.k;
        stage.style.backgroundSize = `${size}px ${size}px`;
        stage.style.backgroundPosition = `${ox}px ${oy}px`;
        setZoom(view.k);
      }),
    [],
  );
  useEffect(() => () => camera.stop(), [camera]);

  const cardView = useCallback(
    (rect: Rect): View => fitView(rect, camera.vw, camera.vh, 56, 1),
    [camera],
  );
  const overview = useCallback(
    (): View =>
      fitView(layoutRef.current.bounds, camera.vw, camera.vh, 64, 1, MIN_K),
    [camera],
  );

  // Track the viewport size; place the camera the first time.
  const placed = useRef(false);
  useLayoutEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      camera.vw = r.width;
      camera.vh = r.height;
      if (!placed.current && r.width > 0 && layoutRef.current.frames.length) {
        placed.current = true;
        const last = model.edits[snap.played - 1];
        const rect = last && layoutRef.current.cards.get(last.path);
        camera.set(rect ? cardView(rect) : overview());
      } else {
        camera.set(camera.view);
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [camera, paths.length > 0]);

  // Follow the agent: fly to each edit's card as it starts.
  const activeEdit = snap.active?.edit;
  useEffect(() => {
    if (!activeEdit || !followingRef.current) return;
    const rect = layout.cards.get(activeEdit.path);
    if (!rect) return;
    placed.current = true;
    camera.flyTo(
      cardView(rect),
      Math.max(420, (snap.active?.travelMs ?? 400) + 120),
    );
  }, [activeEdit]);

  // The latest run stays beside the code it ran until the next edit.
  const lastRun =
    snap.runsShown > 0 ? model.runs[snap.runsShown - 1] : undefined;
  const shownRun =
    lastRun && lastRun.afterEdit >= snap.played && !snap.active
      ? lastRun
      : undefined;
  const runAnchor = shownRun
    ? layout.cards.get(shownRun.target?.path ?? shownRun.touches[0] ?? '')
    : undefined;
  const runRect: Rect | undefined = runAnchor && {
    h: CARD_H,
    w: RUN_W,
    x: runAnchor.x + runAnchor.w + 28,
    y: runAnchor.y,
  };
  const activeRun = snap.activeRun?.run;
  useEffect(() => {
    if (!activeRun || !followingRef.current || !runAnchor || !runRect) return;
    const both = unionRects([runAnchor, runRect])!;
    camera.flyTo(
      fitView(both, camera.vw, camera.vh, 48, 1),
      Math.max(420, (snap.activeRun?.travelMs ?? 400) + 120),
    );
  }, [activeRun]);

  // While nothing is happening, pull back to the whole map.
  const busy = Boolean(activeEdit || activeRun);
  useEffect(() => {
    if (busy || !following || snap.played === 0) return;
    const id = setTimeout(
      () => camera.flyTo(overview(), 1400),
      OVERVIEW_AFTER_MS,
    );
    return () => clearTimeout(id);
  }, [busy, following, snap.played, snap.runsShown, layout]);

  // A file picked in the rail.
  useEffect(() => {
    if (!focusRequest) return;
    const rect = layout.cards.get(focusRequest.path);
    if (!rect) return;
    setFollowing(false);
    camera.flyTo(cardView(rect), 650);
  }, [focusRequest]);

  // Wheel pans (or zooms with ctrl/pinch); inside a readable card it scrolls the code.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const inCode = (e.target as HTMLElement).closest('.aa-pane-body');
      if (inCode && !e.ctrlKey && camera.view.k >= 0.75) return;
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
  }, [camera, paths.length > 0]);

  const drag = useRef<{ x: number; y: number; id: number } | null>(null);
  const onPointerDown = (e: PointerEvent) => {
    if (
      e.button !== 0 ||
      (e.target as HTMLElement).closest('.aa-pane-body, button')
    )
      return;
    drag.current = { id: e.pointerId, x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    if (Math.abs(dx) + Math.abs(dy) < 2) return;
    setFollowing(false);
    camera.panBy(dx, dy);
    drag.current = { ...d, x: e.clientX, y: e.clientY };
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  const trail = useMemo(() => {
    const out: string[] = [];
    for (let i = snap.played - 1; i >= 0 && out.length < TRAIL_LENGTH; i--) {
      const path = model.edits[i].path;
      if (out.at(-1) !== path) out.push(path);
    }
    return out.reverse();
  }, [snap.played, model.version]);

  const activePath = snap.active?.edit.path ?? null;
  const activeDir = activePath
    ? activePath.slice(0, Math.max(0, activePath.lastIndexOf('/')))
    : null;
  const ranPaths = new Set(shownRun?.touches ?? []);

  // Captions sit under the card the agent is on; zoomed out, they sit at the bottom.
  const caption = useCaption();
  const focusPath =
    activePath ?? shownRun?.target?.path ?? model.edits[snap.played - 1]?.path;
  const captionAnchor = focusPath ? layout.cards.get(focusPath) : undefined;
  const anchored = Boolean(caption && captionAnchor && zoom >= 0.6);

  if (paths.length === 0) {
    return (
      <div className="aa-stage" ref={stageRef}>
        <div className="aa-waiting">
          <h2>Waiting for the first edit</h2>
          <p>
            Each file the agent touches appears here as a card, grouped by
            folder. The camera follows the agent.
          </p>
        </div>
        <div ref={worldRef} />
      </div>
    );
  }

  return (
    <div
      className="aa-stage"
      onPointerCancel={onPointerUp}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      ref={stageRef}
    >
      <div className="aa-stage-world" ref={worldRef}>
        {layout.frames.map((f) => (
          <div
            className="aa-frame"
            data-active={f.dir === activeDir}
            key={f.dir}
            style={{ height: f.h, left: f.x, top: f.y, width: f.w }}
          >
            <div className="aa-frame-label">
              <FrameLabel dir={f.dir} />
            </div>
          </div>
        ))}
        <Trail cards={layout.cards} paths={trail} />
        {paths.map((path) => {
          const rect = layout.cards.get(path);
          const last = model.lastEditOf(path, snap.played);
          if (!rect || last === undefined) return null;
          return (
            <StageCard
              active={snap.active?.edit.path === path ? snap.active : null}
              edit={model.edits[last]}
              key={path}
              onFocus={() => {
                setFollowing(false);
                camera.flyTo(cardView(rect), 600);
              }}
              ran={ranPaths.has(path)}
              rect={rect}
            />
          );
        })}
        {shownRun && runRect && runAnchor && (
          <>
            <svg
              aria-hidden="true"
              className="aa-run-link"
              height={1}
              width={1}
            >
              <path
                d={`M${runAnchor.x + runAnchor.w} ${runAnchor.y + 40} H${runRect.x}`}
                stroke={
                  shownRun.status === 'failed'
                    ? 'var(--aa-del)'
                    : 'var(--aa-agent)'
                }
                strokeDasharray="4 4"
                strokeWidth={2}
                vectorEffect="non-scaling-stroke"
              />
            </svg>
            <div
              className="aa-stage-run"
              key={shownRun.id}
              style={{ left: runRect.x, top: runRect.y, width: RUN_W }}
            >
              <RunCard active={activeRun === shownRun} run={shownRun} />
            </div>
          </>
        )}
        {anchored && caption && captionAnchor && (
          <Caption
            fresh={caption.fresh}
            note={caption.note}
            placement="anchored"
            style={{
              left: captionAnchor.x,
              maxWidth: CARD_W + 160,
              top: captionAnchor.y + captionAnchor.h + 18,
            }}
          />
        )}
      </div>
      {caption && !anchored && (
        <Caption fresh={caption.fresh} note={caption.note} />
      )}
      {!following && (
        <button
          className="aa-follow"
          onClick={() => {
            setFollowing(true);
            const target = model.edits[snap.played - 1];
            const rect = target && layout.cards.get(target.path);
            camera.flyTo(rect ? cardView(rect) : overview(), 700);
          }}
          type="button"
        >
          <b>Follow the agent</b>
        </button>
      )}
      <div className="aa-zoom">
        <button
          aria-label="Show the whole project"
          className="aa-icon-button"
          onClick={() => {
            setFollowing(false);
            camera.flyTo(overview(), 700);
          }}
          title="Show everything"
          type="button"
        >
          {fitIcon}
        </button>
        <span>{Math.round(zoom * 100)}%</span>
      </div>
    </div>
  );
}

function FrameLabel({ dir }: { dir: string }) {
  if (!dir) return <b>./</b>;
  const slash = dir.lastIndexOf('/');
  return (
    <>
      {slash >= 0 ? dir.slice(0, slash + 1) : ''}
      <b>{dir.slice(slash + 1)}</b>
    </>
  );
}

function Trail({
  paths,
  cards,
}: {
  paths: string[];
  cards: Map<string, Rect>;
}) {
  const points = paths
    .map((p) => cards.get(p))
    .filter((r): r is Rect => Boolean(r));
  if (points.length < 2) return null;
  const segments = [];
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const b = points[i];
    const ax = a.x + a.w / 2;
    const ay = a.y + a.h / 2;
    const bx = b.x + b.w / 2;
    const by = b.y + b.h / 2;
    // Bend each hop slightly so a return trip does not retrace the same line.
    const mx = (ax + bx) / 2 - (by - ay) * 0.12;
    const my = (ay + by) / 2 + (bx - ax) * 0.12;
    const age = points.length - 1 - i;
    segments.push(
      <path
        d={`M${ax},${ay} Q${mx},${my} ${bx},${by}`}
        fill="none"
        key={i}
        opacity={Math.max(0.12, 0.85 - age * 0.14)}
        stroke="var(--aa-agent)"
        strokeDasharray={age === 0 ? undefined : '6 6'}
        strokeLinecap="round"
        strokeWidth={2}
        vectorEffect="non-scaling-stroke"
      />,
    );
  }
  return (
    <svg aria-hidden="true" className="aa-route" height={1} width={1}>
      {segments}
    </svg>
  );
}

const StageCard = memo(function StageCard({
  rect,
  edit,
  active,
  ran,
  onFocus,
}: {
  rect: Rect;
  edit: Edit;
  active: ActiveEdit | null;
  ran: boolean;
  onFocus: () => void;
}) {
  const model = useModel();
  const snap = usePlayer();
  const stats = model.sessionStats(edit.path, snap.played);
  const { name } = splitPath(edit.path);
  const isImage = Boolean(edit.beforeMedia || edit.afterMedia);
  const [entering, setEntering] = useState(true);
  useEffect(() => {
    const id = setTimeout(() => setEntering(false), 600);
    return () => clearTimeout(id);
  }, []);

  return (
    <div
      className="aa-card"
      data-entering={entering}
      data-ran={ran}
      onDoubleClick={onFocus}
      style={{ height: CARD_H, left: rect.x, top: rect.y, width: CARD_W }}
    >
      <section
        aria-label={edit.path}
        className="aa-pane"
        data-active={active !== null}
      >
        <header className="aa-pane-head">
          <span className="aa-pane-path" title={edit.path}>
            <span className="name">{name}</span>
          </span>
          <span className="aa-pane-meta">
            {active ? <span className="aa-writing">writing</span> : null}
            <Counts added={stats.added} removed={stats.removed} />
            <span>
              {stats.edits} {stats.edits === 1 ? 'edit' : 'edits'}
            </span>
          </span>
        </header>
        <EditCaptionLine edit={edit} />
        <div className="aa-card-body" data-media={isImage || undefined}>
          <div className="aa-pane-body">
            <EditDiff active={active} context={6} edit={edit} />
          </div>
          {!isImage && <Minimap edit={edit} />}
        </div>
      </section>
    </div>
  );
});

/** The whole file as a silhouette, with the latest edit marked. */
const Minimap = memo(function Minimap({ edit }: { edit: Edit }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth * dpr;
    const h = canvas.clientHeight * dpr;
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const styles = getComputedStyle(canvas);
    const ink = styles.getPropertyValue('--aa-text-3').trim() || '#6d7a93';
    const add = styles.getPropertyValue('--aa-add').trim() || '#47c68f';
    const del = styles.getPropertyValue('--aa-del').trim() || '#ee6f78';
    const lines = (edit.after ?? '').split('\n');
    const inserted = new Set<number>();
    for (const op of edit.ops)
      if (op.type === 'insert') inserted.add(op.newIndex);
    const deletedAt: number[] = [];
    for (const hunk of edit.hunks) {
      if (hunk.removed > 0 && hunk.added === 0)
        deletedAt.push(hunk.newStart + Math.floor(hunk.newCount / 2));
    }
    const rowH = Math.min(3 * dpr, h / Math.max(1, lines.length));
    const charW = w / 90;
    ctx.clearRect(0, 0, w, h);
    lines.forEach((line, i) => {
      const len = line.trim().length;
      if (!len) return;
      const indent = line.length - line.trimStart().length;
      const changed = inserted.has(i);
      ctx.globalAlpha = changed ? 0.95 : 0.35;
      ctx.fillStyle = changed ? add : ink;
      ctx.fillRect(
        Math.min(w - 2, indent * charW),
        i * rowH,
        Math.max(2, Math.min(w, len * charW)),
        Math.max(1, rowH * 0.7),
      );
    });
    ctx.globalAlpha = 1;
    ctx.fillStyle = del;
    for (const i of deletedAt) ctx.fillRect(0, i * rowH, w, Math.max(2, rowH));
  }, [edit]);
  return (
    <div aria-hidden="true" className="aa-minimap">
      <canvas ref={ref} />
    </div>
  );
});

const fitIcon = (
  <svg
    aria-hidden="true"
    fill="none"
    height="14"
    viewBox="0 0 16 16"
    width="14"
  >
    <path
      d="M2 6V3a1 1 0 0 1 1-1h3M10 2h3a1 1 0 0 1 1 1v3M14 10v3a1 1 0 0 1-1 1h-3M6 14H3a1 1 0 0 1-1-1v-3"
      stroke="currentColor"
      strokeLinecap="round"
      strokeWidth="1.5"
    />
  </svg>
);
