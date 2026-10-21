// One edit rendered by @pierre/diffs, optionally animated as it happens.
//
// The animation is CSS inside the diff's shadow root: removed rows collapse
// into a thin seam, then added rows open one after another, each row whole.
// After every render the rows are numbered
// (`--aa-k`) so each can be delayed by its position, and the host carries the
// timing. A re-render midway (highlighting arrives asynchronously) restarts
// the CSS animations, so the host also carries how far along the edit already
// is, and every delay is shifted back by that much.

import type { FileContents } from '@pierre/diffs';
import { MultiFileDiff } from '@pierre/diffs/react';
import { type CSSProperties, memo, useEffect, useMemo, useRef } from 'react';
import { ensureDiffsSetup } from '../diffs-setup';
import type { Edit } from '../engine/model';
import type { ActiveEdit } from '../engine/player';
import { ImageDiff } from './image-diff';

ensureDiffsSetup();

const ANIMATION_CSS = `
:host([data-aa-anim]) [data-line-type='change-deletion'],
:host([data-aa-anim]) [data-line-type='change-addition'] {
  overflow: hidden;
}

/* Removed rows: flash, then collapse to a seam that marks where text left. */
:host([data-aa-anim='play']) [data-line-type='change-deletion'] {
  animation: aa-del var(--aa-del-dur) cubic-bezier(0.5, 0, 0.7, 0.4) both;
  animation-delay: calc(var(--aa-travel) + var(--aa-k, 0) * var(--aa-del-stagger) + var(--aa-shift, 0ms));
}
:host([data-aa-anim='settled']) [data-line-type='change-deletion'] {
  height: 3px;
  min-height: 3px;
  color: transparent;
}
:host([data-aa-anim='settled']) [data-content] > [data-line-type='change-deletion'] *,
:host([data-aa-anim='settled']) [data-gutter] > [data-line-type='change-deletion'] * {
  visibility: hidden;
}
@keyframes aa-del {
  0% { height: var(--diffs-line-height); min-height: var(--diffs-line-height); }
  40% { height: var(--diffs-line-height); min-height: var(--diffs-line-height); filter: brightness(1.35); }
  100% { height: 3px; min-height: 3px; color: transparent; }
}

/* Added rows: each row opens whole, one after another. Rows never reveal
   left to right; staggered horizontal reveals read as a diagonal wipe. */
:host([data-aa-anim='play']) [data-gutter] > [data-line-type='change-addition'],
:host([data-aa-anim='play']) [data-content] > [data-line-type='change-addition'] {
  animation: aa-add-row var(--aa-add-dur) cubic-bezier(0.2, 0.7, 0.2, 1) both;
  animation-delay: calc(var(--aa-add-start) + var(--aa-k, 0) * var(--aa-add-stagger) + var(--aa-shift, 0ms));
}
@keyframes aa-add-row {
  from { height: 0; min-height: 0; opacity: 0; }
  60% { opacity: 1; }
  to { height: var(--diffs-line-height); min-height: var(--diffs-line-height); opacity: 1; }
}
`;

/** A line in an edit's diff: `new` lines are in the edit's result, `old` lines were removed. 0-based. */
export interface DiffLine {
  side: 'old' | 'new';
  line: number;
}

export interface EditDiffProps {
  edit: Edit;
  /** Pointer events on lines, for linking code to other views. */
  onLine?: (line: DiffLine | null, action: 'hover' | 'click') => void;
  /** When this is the edit being played, the timing to animate it with. */
  active?: ActiveEdit | null;
  /** Unchanged lines shown around each change. */
  context?: number;
  /** `settled` shows the result of the edit; `plain` shows an ordinary diff. */
  presentation?: 'settled' | 'plain';
  /** Keep the changed rows in view while the edit plays. */
  followTyping?: boolean;
  style?: CSSProperties;
}

const DIFF_STYLE = {
  '--diffs-dark-bg': 'var(--aa-raised)',
  '--diffs-light-bg': 'var(--aa-raised)',
} as CSSProperties;

export const EditDiff = memo(function EditDiff({
  edit,
  active,
  context = 4,
  presentation = 'settled',
  followTyping = true,
  style,
  onLine,
}: EditDiffProps) {
  const wrapperRef = useRef<HTMLDivElement>(null);
  const playing = active && active.edit === edit ? active : null;
  const playingRef = useRef(playing);
  playingRef.current = playing;
  // Options must stay referentially stable, so line handlers read the latest callback.
  const onLineRef = useRef(onLine);
  onLineRef.current = onLine;

  const oldFile = useMemo<FileContents | null>(
    () =>
      edit.before === null
        ? null
        : {
            cacheKey: `${edit.seq}:before`,
            contents: edit.before,
            name: edit.path,
          },
    [edit],
  );
  const newFile = useMemo<FileContents | null>(
    () =>
      edit.after === null
        ? null
        : {
            cacheKey: `${edit.seq}:after`,
            contents: edit.after,
            name: edit.path,
          },
    [edit],
  );

  const options = useMemo(
    () => ({
      diffStyle: 'unified' as const,
      disableFileHeader: true,
      hunkSeparators: 'line-info-basic' as const,
      lineDiffType: 'word-alt' as const,
      onLineClick: (p: {
        lineNumber: number;
        annotationSide: 'deletions' | 'additions';
      }) => onLineRef.current?.(toDiffLine(p), 'click'),
      onLineEnter: (p: {
        lineNumber: number;
        annotationSide: 'deletions' | 'additions';
      }) => onLineRef.current?.(toDiffLine(p), 'hover'),
      onLineLeave: () => onLineRef.current?.(null, 'hover'),
      onPostRender: (node: HTMLElement) => {
        prepareRows(node, playingRef.current);
      },
      overflow: 'scroll' as const,
      parseDiffOptions: { context },
      theme: { dark: 'pierre-dark', light: 'pierre-light' },
      themeType: 'system' as const,
      unsafeCSS: ANIMATION_CSS,
    }),
    [context],
  );

  // Set the animation mode on the host whenever the playing edit changes.
  useEffect(() => {
    const host = wrapperRef.current?.querySelector(
      'diffs-container',
    ) as HTMLElement | null;
    if (!host) return;
    if (playing) prepareRows(host, playing);
    else setMode(host, presentation === 'settled' ? 'settled' : null);
  }, [playing, presentation]);

  // Keep the row being typed in view.
  useEffect(() => {
    if (!playing || !followTyping) return;
    const scroller = wrapperRef.current;
    if (!scroller) return;
    let frame = 0;
    const tick = () => {
      const host = scroller.querySelector(
        'diffs-container',
      ) as HTMLElement | null;
      const root = host?.shadowRoot;
      const t = performance.now() - playing.startedAt;
      if (root && t < playing.travelMs + playing.writeMs + 200) {
        const rows = root.querySelectorAll<HTMLElement>(
          "[data-content] > [data-line-type='change-addition'], [data-content] > [data-line-type='change-deletion']",
        );
        const timing = rowTiming(playing, root);
        let target: HTMLElement | undefined;
        if (t < playing.travelMs) target = rows[0];
        else {
          const k = Math.floor(
            (t - timing.addStart) / Math.max(1, timing.addStagger),
          );
          const adds = root.querySelectorAll<HTMLElement>(
            "[data-content] > [data-line-type='change-addition']",
          );
          target = adds[Math.max(0, Math.min(adds.length - 1, k))] ?? rows[0];
        }
        if (target) {
          const box = scroller.getBoundingClientRect();
          const r = target.getBoundingClientRect();
          const desired =
            scroller.scrollTop + (r.top - box.top) - box.height * 0.45;
          scroller.scrollTop += (desired - scroller.scrollTop) * 0.18;
        }
        frame = requestAnimationFrame(tick);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing, followTyping]);

  if (edit.beforeMedia || edit.afterMedia) {
    return (
      <div className="aa-diff" style={style}>
        <ImageDiff
          after={edit.afterMedia}
          before={edit.beforeMedia}
          name={edit.path.slice(edit.path.lastIndexOf('/') + 1)}
          playing={playing}
          size={edit.size}
        />
      </div>
    );
  }
  if (edit.opaque) {
    return (
      <div className="aa-diff aa-diff-opaque">
        {edit.kind === 'deleted'
          ? 'Deleted.'
          : edit.tooLarge
            ? 'Too large to show.'
            : 'Binary file; only images are shown.'}
      </div>
    );
  }
  if (oldFile === null && newFile === null) {
    return <div className="aa-diff aa-diff-opaque">Empty file.</div>;
  }

  return (
    <div className="aa-diff" ref={wrapperRef} style={style}>
      {oldFile === null ? (
        <MultiFileDiff
          newFile={newFile as FileContents}
          oldFile={null}
          options={options}
          style={DIFF_STYLE}
        />
      ) : newFile === null ? (
        <MultiFileDiff
          newFile={null}
          oldFile={oldFile}
          options={options}
          style={DIFF_STYLE}
        />
      ) : (
        <MultiFileDiff
          newFile={newFile}
          oldFile={oldFile}
          options={options}
          style={DIFF_STYLE}
        />
      )}
    </div>
  );
});

function toDiffLine(p: {
  lineNumber: number;
  annotationSide: 'deletions' | 'additions';
}): DiffLine {
  return {
    line: p.lineNumber - 1,
    side: p.annotationSide === 'deletions' ? 'old' : 'new',
  };
}

function setMode(host: HTMLElement, mode: 'play' | 'settled' | null) {
  if (mode) host.setAttribute('data-aa-anim', mode);
  else host.removeAttribute('data-aa-anim');
}

interface RowTiming {
  delStagger: number;
  delDur: number;
  addStart: number;
  addStagger: number;
  addDur: number;
}

function rowTiming(
  active: ActiveEdit,
  root: ShadowRoot | HTMLElement,
): RowTiming {
  const dels = root.querySelectorAll(
    "[data-content] > [data-line-type='change-deletion']",
  ).length;
  const adds = root.querySelectorAll(
    "[data-content] > [data-line-type='change-addition']",
  ).length;
  const write = active.writeMs;
  // Deletions take at most a third of the write, additions the rest.
  const delBudget = dels ? write * (adds ? 0.32 : 0.8) : 0;
  const delDur = Math.min(420, delBudget * 0.7);
  const delStagger =
    dels > 1 ? Math.min(30, (delBudget - delDur) / (dels - 1)) : 0;
  const addBudget = write - delBudget;
  const addDur = Math.min(260, addBudget * 0.5);
  const addStagger =
    adds > 1 ? Math.min(46, (addBudget - addDur) / (adds - 1)) : 0;
  return {
    addDur,
    addStagger,
    addStart: active.travelMs + delBudget,
    delDur,
    delStagger,
  };
}

/** Number the changed rows and put the edit's timing on the host. */
function prepareRows(node: HTMLElement, active: ActiveEdit | null) {
  const host = (
    node.tagName.toLowerCase() === 'diffs-container'
      ? node
      : node.closest('diffs-container')
  ) as HTMLElement | null;
  const root = host?.shadowRoot;
  if (!host || !root) return;
  if (!active) {
    if (host.getAttribute('data-aa-anim') === 'play') setMode(host, 'settled');
    return;
  }
  const elapsed = performance.now() - active.startedAt;
  const total = active.travelMs + active.writeMs;
  if (elapsed > total + 200) {
    setMode(host, 'settled');
    return;
  }
  for (const type of ['change-deletion', 'change-addition']) {
    const rows = root.querySelectorAll<HTMLElement>(
      `[data-content] > [data-line-type='${type}']`,
    );
    rows.forEach((row, k) => {
      row.style.setProperty('--aa-k', String(k));
      const index = row.getAttribute('data-line-index');
      if (index) {
        const gutter = root.querySelector<HTMLElement>(
          `[data-gutter] > [data-line-index='${index}']`,
        );
        gutter?.style.setProperty('--aa-k', String(k));
      }
    });
  }
  const t = rowTiming(active, root);
  host.style.setProperty('--aa-travel', `${active.travelMs}ms`);
  host.style.setProperty('--aa-del-stagger', `${t.delStagger}ms`);
  host.style.setProperty('--aa-del-dur', `${t.delDur}ms`);
  host.style.setProperty('--aa-add-start', `${t.addStart}ms`);
  host.style.setProperty('--aa-add-stagger', `${t.addStagger}ms`);
  host.style.setProperty('--aa-add-dur', `${t.addDur}ms`);
  host.style.setProperty('--aa-shift', `${-elapsed}ms`);
  // Restart the animations from the shifted origin.
  host.removeAttribute('data-aa-anim');
  void host.offsetWidth;
  setMode(host, 'play');
}
