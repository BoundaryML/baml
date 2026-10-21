// The agent's own words, from its transcript, shown like subtitles under the
// work they describe. Captions are optional: they need a transcript the watch
// server can read, and the toggle in the menu bar turns them off.

import { type CSSProperties, type ReactNode, useState } from 'react';
import type { Edit, Note } from '../engine/model';
import { useModel, usePlayer, useRuntime, useTicker } from '../runtime';
import { formatClock, splitPath, withOffsets } from './symbols';

/** Captions older than this (session time) fade back. */
const FRESH_MS = 25_000;

const AGENT_NAMES: Record<string, string> = {
  'claude-code': 'Claude',
  codex: 'Codex',
};

/** The caption for the current moment, or null when captions are off or none exist. */
export function useCaption(): { note: Note; fresh: boolean } | null {
  const model = useModel();
  const snap = usePlayer();
  const { captions, source } = useRuntime();
  useTicker(1000);
  if (!captions) return null;
  const notes = model.notesAt(snap.played);
  const note = notes.at(-1);
  if (!note) return null;
  return { fresh: source.now() - note.time < FRESH_MS, note };
}

export function Caption({
  note,
  fresh,
  placement = 'overlay',
  style,
}: {
  note: Note;
  fresh: boolean;
  placement?: 'overlay' | 'anchored';
  style?: CSSProperties;
}) {
  return (
    <div
      className="aa-caption"
      data-fresh={fresh}
      data-placement={placement}
      key={note.id}
      style={style}
    >
      <span className="aa-caption-who">
        {AGENT_NAMES[note.agent ?? ''] ?? note.agent ?? 'Agent'}
      </span>
      <span className="aa-caption-text">{renderInlineCode(note.text)}</span>
    </div>
  );
}

/** The caption bar at the bottom of a view. */
export function CaptionBar() {
  const caption = useCaption();
  if (!caption) return null;
  return <Caption fresh={caption.fresh} note={caption.note} />;
}

/**
 * The last thing the agent said before `edit`, as one line under a pane
 * header. Hovering it lists every caption for the file.
 */
export function EditCaptionLine({ edit }: { edit: Edit }) {
  const model = useModel();
  const { captions } = useRuntime();
  if (!captions) return null;
  const note = model.noteBefore(edit.index);
  if (!note) return null;
  return (
    <HoverHistory className="aa-edit-caption" path={edit.path}>
      <span className="aa-caption-who">
        {AGENT_NAMES[note.agent ?? ''] ?? 'Agent'}
      </span>
      <span className="aa-edit-caption-text">
        {renderInlineCode(note.text)}
      </span>
    </HoverHistory>
  );
}

/**
 * Wraps `children`; on hover, shows every caption for `path` in a card beside
 * it. The card is fixed-positioned so scrolling rails and scaled canvases
 * cannot clip it.
 */
export function HoverHistory({
  path,
  className,
  children,
}: {
  path: string;
  className?: string;
  children: ReactNode;
}) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const { captions } = useRuntime();
  return (
    <div
      className={className}
      onMouseEnter={(e) =>
        captions && setAnchor(e.currentTarget.getBoundingClientRect())
      }
      onMouseLeave={() => setAnchor(null)}
    >
      {children}
      {anchor && <CaptionHistory anchor={anchor} path={path} />}
    </div>
  );
}

function CaptionHistory({ path, anchor }: { path: string; anchor: DOMRect }) {
  const model = useModel();
  const snap = usePlayer();
  const items = model.notesForFile(path, snap.played);
  if (!items.length) return null;
  const width = 380;
  const right = anchor.right + 10 + width < window.innerWidth;
  const left = right
    ? anchor.right + 10
    : Math.max(8, anchor.left - width - 10);
  const top = Math.min(Math.max(8, anchor.top - 8), window.innerHeight - 60);
  return (
    <div className="aa-history" role="tooltip" style={{ left, top, width }}>
      <div className="aa-history-title">
        What the agent said about <b>{splitPath(path).name}</b>
      </div>
      <ol>
        {items.map(({ note, edits }) => (
          <li key={note.id}>
            <span className="aa-history-meta">
              {formatClock(note.time)}, before{' '}
              {edits.length === 1 ? 'edit' : 'edits'}{' '}
              {edits.map((i) => i + 1).join(', ')}
            </span>
            <span>{renderInlineCode(note.text)}</span>
          </li>
        ))}
      </ol>
    </div>
  );
}

function renderInlineCode(text: string) {
  return withOffsets(text.split(/(`[^`]+`)/)).map(({ part, offset }) =>
    part.startsWith('`') && part.endsWith('`') ? (
      <code key={offset}>{part.slice(1, -1)}</code>
    ) : (
      part
    ),
  );
}
