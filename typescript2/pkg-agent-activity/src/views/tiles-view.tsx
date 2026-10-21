// Tiles: the files being edited, tiled like terminal panes in tmux's
// main-vertical layout. The file being written takes the large pane; the most
// recently edited others stack to its right. When the agent runs something,
// the file it ran takes the large pane and the run's output takes the top of
// the stack. When the agent moves to another file, the panes slide into their
// new places rather than being redrawn, so the eye can follow where each file
// went.

import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Caption, EditCaptionLine, useCaption } from '../components/caption';
import { EditDiff } from '../components/edit-diff';
import { RunCard } from '../components/run-card';
import {
  ChangeTag,
  Counts,
  displayName,
  KindBadge,
  splitPath,
} from '../components/symbols';
import type { Edit, Run } from '../engine/model';
import type { ActiveEdit } from '../engine/player';
import { useModel, usePlayer, useRuntime } from '../runtime';

const GAP = 10;
const MIN_STACK_HEIGHT = 170;

interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function TilesView() {
  const model = useModel();
  const snap = usePlayer();
  const { focusRequest } = useRuntime();
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ h: 700, w: 1000 });
  const seen = useRef(new Set<string>());

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      setSize({ h: r.height, w: r.width });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const maxStack = Math.max(
    1,
    Math.min(4, Math.floor((size.h + GAP) / (MIN_STACK_HEIGHT + GAP))),
  );

  // The latest run gets a pane while it is the most recent thing that happened.
  const lastRun =
    snap.runsShown > 0 ? model.runs[snap.runsShown - 1] : undefined;
  const shownRun =
    lastRun && lastRun.afterEdit >= snap.played && !snap.active
      ? lastRun
      : undefined;

  // Most recently active first: edits, and the file a run executed. A file
  // picked in the rail jumps to the front.
  const order = useMemo(() => {
    const lastByPath = new Map<string, number>();
    for (let i = 0; i < snap.played; i++)
      lastByPath.set(model.edits[i].path, model.edits[i].time);
    const runPath = shownRun?.target?.path ?? shownRun?.touches[0];
    if (shownRun && runPath && lastByPath.has(runPath))
      lastByPath.set(runPath, shownRun.startedAt);
    const paths = [...lastByPath.keys()].sort(
      (a, b) => lastByPath.get(b)! - lastByPath.get(a)!,
    );
    if (focusRequest && lastByPath.has(focusRequest.path) && !snap.active) {
      paths.splice(paths.indexOf(focusRequest.path), 1);
      paths.unshift(focusRequest.path);
    }
    const tiles: Tile[] = paths.map((path) => {
      const last = model.lastEditOf(path, snap.played)!;
      return { edit: model.edits[last], key: path, kind: 'file' };
    });
    if (shownRun)
      tiles.splice(Math.min(1, tiles.length), 0, {
        key: 'run',
        kind: 'run',
        run: shownRun,
      });
    return tiles.slice(0, 1 + maxStack);
  }, [
    snap.played,
    snap.runsShown,
    model.version,
    maxStack,
    focusRequest,
    snap.active,
    shownRun,
  ]);

  const caption = useCaption();
  const height = caption ? size.h - CAPTION_SPACE : size.h;
  const rects = layoutTiles(order.length, size.w, height);
  const entering = new Set(
    order.map((o) => o.key).filter((k) => !seen.current.has(k)),
  );
  useLayoutEffect(() => {
    for (const o of order) seen.current.add(o.key);
  });

  return (
    <div className="aa-tiles" ref={ref}>
      {order.length === 0 && (
        <div className="aa-waiting">
          <h2>Waiting for the first edit</h2>
          <p>
            Each file the agent edits gets a pane. The one it is writing in
            takes the large pane.
          </p>
        </div>
      )}
      {order.map((tile, i) => (
        <div
          className="aa-tile"
          data-entering={entering.has(tile.key)}
          key={tile.key}
          style={rects[i]}
        >
          {tile.kind === 'file' ? (
            <Pane
              active={snap.active}
              edit={tile.edit}
              height={rects[i].height}
              index={i + 1}
              main={i === 0}
            />
          ) : (
            <div className="aa-run-pane">
              <RunCard
                active={snap.activeRun?.run === tile.run}
                run={tile.run}
              />
            </div>
          )}
        </div>
      ))}
      {caption && <Caption fresh={caption.fresh} note={caption.note} />}
    </div>
  );
}

type Tile =
  | { key: string; kind: 'file'; edit: Edit }
  | { key: string; kind: 'run'; run: Run };

/** Room kept under the panes for the caption. */
const CAPTION_SPACE = 64;

function layoutTiles(n: number, w: number, h: number): Rect[] {
  if (n <= 1) return [{ height: h, left: 0, top: 0, width: w }];
  const mainW = Math.round(w * 0.6 - GAP / 2);
  const stackW = w - mainW - GAP;
  const m = n - 1;
  const stackH = (h - GAP * (m - 1)) / m;
  const rects: Rect[] = [{ height: h, left: 0, top: 0, width: mainW }];
  for (let i = 0; i < m; i++) {
    rects.push({
      height: stackH,
      left: mainW + GAP,
      top: i * (stackH + GAP),
      width: stackW,
    });
  }
  return rects;
}

function Pane({
  index,
  edit,
  active,
  main,
  height,
}: {
  index: number;
  edit: Edit;
  active: ActiveEdit | null;
  main: boolean;
  height: number;
}) {
  const model = useModel();
  const snap = usePlayer();
  const isActive = active?.edit.path === edit.path;
  const stats = model.sessionStats(edit.path, snap.played);
  const { dir, name } = splitPath(edit.path);
  const visibleLines = Math.floor((height - 32 - 34) / 19);
  const changed = edit.added + edit.removed;
  const context = main
    ? Math.max(
        3,
        Math.min(
          14,
          Math.floor((visibleLines - Math.min(changed, visibleLines)) / 2),
        ),
      )
    : 2;
  const symbols = edit.symbols.filter((s) => s.change !== 'body').slice(0, 4);
  const bodies = edit.symbols
    .filter((s) => s.change === 'body')
    .slice(0, Math.max(0, 3 - symbols.length));

  return (
    <section aria-label={edit.path} className="aa-pane" data-active={isActive}>
      <header className="aa-pane-head">
        <span className="aa-pane-index">{index}</span>
        <span className="aa-pane-path" title={edit.path}>
          {/* The directory elides from the left (rtl); the marks keep its slashes in place. */}
          <span className="dir">{`‎${dir}‎`}</span>
          <span className="name">{name}</span>
        </span>
        <span className="aa-pane-meta">
          {isActive ? <span className="aa-writing">writing</span> : null}
          <Counts added={stats.added} removed={stats.removed} />
          <span>
            {stats.edits} {stats.edits === 1 ? 'edit' : 'edits'}
          </span>
        </span>
      </header>
      <EditCaptionLine edit={edit} />
      <div className="aa-pane-body">
        <EditDiff
          active={isActive ? active : null}
          context={context}
          edit={edit}
        />
      </div>
      {(symbols.length > 0 || bodies.length > 0) && (
        <footer className="aa-pane-foot">
          {[...symbols, ...bodies].map((s) => (
            <span
              className="aa-sym-chip"
              data-change={s.change}
              key={s.key}
              title={`${s.kind} ${s.name}`}
            >
              <KindBadge kind={s.kind} />
              {displayName(s.name, s.container)}
              {main && s.change !== 'body' ? null : null}
            </span>
          ))}
          {main && symbols[0] && <ChangeTag change={symbols[0].change} />}
        </footer>
      )}
    </section>
  );
}
