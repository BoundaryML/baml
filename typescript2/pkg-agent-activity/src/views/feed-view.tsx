// Feed: everything that happened, in order, grouped into bursts, newest burst
// first. Edits render as diffs, runs as their result, and the agent's own
// messages (when captions are on) sit between them the way it said them.
// "Since start" instead shows one diff per file against the session baseline:
// what a reviewer would read.

import type { FileContents } from '@pierre/diffs';
import { MultiFileDiff } from '@pierre/diffs/react';
import { type CSSProperties, memo, useMemo, useState } from 'react';
import { EditDiff } from '../components/edit-diff';
import { ImageDiff } from '../components/image-diff';
import { RunCard } from '../components/run-card';
import {
  Counts,
  formatClock,
  formatDuration,
  splitPath,
} from '../components/symbols';
import { ensureDiffsSetup } from '../diffs-setup';
import { BURST_GAP_MS, type Edit, type Note, type Run } from '../engine/model';
import type { ActiveEdit } from '../engine/player';
import type { MediaRef } from '../engine/protocol';
import { useModel, usePlayer, useRuntime } from '../runtime';

ensureDiffsSetup();

type Item =
  | { kind: 'edit'; time: number; edit: Edit }
  | { kind: 'run'; time: number; run: Run }
  | { kind: 'note'; time: number; note: Note };

const TURNS_PER_PAGE = 12;

export function FeedView() {
  const [mode, setMode] = useState<'edits' | 'session'>('edits');
  return (
    <div className="aa-feed">
      <div className="aa-feed-inner">
        <div className="aa-feed-toolbar">
          <fieldset
            aria-label="Feed mode"
            className="aa-segmented aa-segmented--small"
          >
            <button
              aria-pressed={mode === 'edits'}
              onClick={() => setMode('edits')}
              type="button"
            >
              Each edit
            </button>
            <button
              aria-pressed={mode === 'session'}
              onClick={() => setMode('session')}
              type="button"
            >
              Since start
            </button>
          </fieldset>
        </div>
        {mode === 'edits' ? <EditFeed /> : <SessionDiffs />}
      </div>
    </div>
  );
}

function EditFeed() {
  const model = useModel();
  const snap = usePlayer();
  const { captions } = useRuntime();
  const [pages, setPages] = useState(1);

  const turns = useMemo(() => {
    const items: Item[] = [];
    const runs = model.runs.slice(0, snap.runsShown);
    const notes = captions ? model.notesAt(snap.played) : [];
    let r = 0;
    let n = 0;
    const flushBefore = (editIndex: number) => {
      while (n < notes.length && notes[n].afterEdit <= editIndex) {
        items.push({ kind: 'note', note: notes[n], time: notes[n].time });
        n++;
      }
      while (r < runs.length && runs[r].afterEdit <= editIndex) {
        items.push({ kind: 'run', run: runs[r], time: runs[r].startedAt });
        r++;
      }
    };
    for (let i = 0; i < snap.played; i++) {
      flushBefore(i);
      items.push({
        edit: model.edits[i],
        kind: 'edit',
        time: model.edits[i].time,
      });
    }
    flushBefore(snap.played);
    // Items closer than the burst gap belong to one turn.
    const out: Item[][] = [];
    for (const item of items) {
      const turn = out.at(-1);
      const previous = turn?.at(-1);
      if (turn && previous && item.time - previous.time <= BURST_GAP_MS)
        turn.push(item);
      else out.push([item]);
    }
    return out.reverse();
  }, [model.version, snap.played, snap.runsShown, captions]);

  if (turns.length === 0) {
    return (
      <div className="aa-empty aa-feed-empty">
        Edits appear here as diffs, newest first.
      </div>
    );
  }

  const shown = turns.slice(0, pages * TURNS_PER_PAGE);
  return (
    <>
      {shown.map((turn) => {
        const first = turn[0];
        const last = turn.at(-1) ?? first;
        const edits = turn.filter((i) => i.kind === 'edit').length;
        const files = new Set(
          turn.flatMap((i) => (i.kind === 'edit' ? [i.edit.path] : [])),
        ).size;
        return (
          <section className="aa-turn" key={`${first.kind}${first.time}`}>
            <header className="aa-turn-head">
              {formatClock(first.time)}
              <span>
                {edits > 0
                  ? `${edits} ${edits === 1 ? 'edit' : 'edits'} to ${files} ${files === 1 ? 'file' : 'files'}`
                  : 'No edits'}
                {last.time - first.time > 1000
                  ? ` over ${formatDuration(last.time - first.time)}`
                  : ''}
              </span>
            </header>
            {turn.map((item) =>
              item.kind === 'edit' ? (
                <FeedEdit
                  active={snap.active}
                  edit={item.edit}
                  key={`e${item.edit.index}`}
                />
              ) : item.kind === 'run' ? (
                <div className="aa-feed-run" key={`r${item.run.id}`}>
                  <RunCard
                    active={snap.activeRun?.run === item.run}
                    run={item.run}
                  />
                </div>
              ) : (
                <p className="aa-feed-note" key={item.note.id}>
                  {item.note.text}
                </p>
              ),
            )}
          </section>
        );
      })}
      {turns.length > shown.length && (
        <button
          className="aa-feed-more"
          onClick={() => setPages((p) => p + 1)}
          type="button"
        >
          Show earlier bursts
        </button>
      )}
    </>
  );
}

const FeedEdit = memo(function FeedEdit({
  edit,
  active,
}: {
  edit: Edit;
  active: ActiveEdit | null;
}) {
  const isActive = active?.edit === edit;
  const { dir, name } = splitPath(edit.path);
  return (
    <article
      className="aa-feed-edit"
      data-active={isActive}
      data-entering={isActive}
    >
      <header className="aa-feed-edit-head">
        <span className="aa-pane-path">
          <span className="dir">{`‎${dir}‎`}</span>
          <span className="name">{name}</span>
        </span>
        {edit.kind !== 'modified' && (
          <span
            className="aa-change-tag"
            data-change={edit.kind === 'created' ? 'added' : 'removed'}
          >
            {edit.kind}
          </span>
        )}
        <Counts added={edit.added} removed={edit.removed} />
        <span className="time">{formatClock(edit.time)}</span>
      </header>
      <EditDiff
        active={isActive ? active : null}
        context={3}
        edit={edit}
        followTyping={false}
        presentation="plain"
      />
    </article>
  );
});

const SESSION_DIFF_STYLE = {
  '--diffs-dark-bg': 'var(--aa-raised)',
  '--diffs-light-bg': 'var(--aa-raised)',
} as CSSProperties;

const SESSION_OPTIONS = {
  diffStyle: 'unified' as const,
  disableFileHeader: true,
  hunkSeparators: 'line-info-basic' as const,
  lineDiffType: 'word-alt' as const,
  overflow: 'scroll' as const,
  theme: { dark: 'pierre-dark', light: 'pierre-light' },
  themeType: 'system' as const,
};

/** One diff per touched file: baseline to now, most recently edited first. */
function SessionDiffs() {
  const model = useModel();
  const snap = usePlayer();
  const files = useMemo(
    () =>
      model
        .touchedFiles(snap.played)
        .map((f) => ({
          last: model.lastEditOf(f.path, snap.played) ?? -1,
          path: f.path,
        }))
        .sort((a, b) => b.last - a.last),
    [model.version, snap.played],
  );
  return (
    <>
      {files.map(({ path }) => (
        <SessionDiff
          after={model.contentAt(path, snap.played)}
          afterMedia={model.mediaAt(path, snap.played)}
          before={model.files.get(path)?.baseline ?? null}
          beforeMedia={model.files.get(path)?.baselineMedia ?? null}
          existed={model.existedAtStart(path)}
          gone={model.deletedAt(path, snap.played)}
          key={path}
          opaque={opaqueNote(
            model.edits[model.lastEditOf(path, snap.played) ?? -1],
          )}
          path={path}
        />
      ))}
    </>
  );
}

/** Why a file's latest content is not shown as text, if it is not. */
function opaqueNote(edit: Edit | undefined): string | null {
  if (!edit?.opaque || edit.kind === 'deleted') return null;
  return edit.tooLarge
    ? 'Too large to show.'
    : 'Binary file; only images are shown.';
}

const SessionDiff = memo(function SessionDiff({
  path,
  before,
  after,
  beforeMedia,
  afterMedia,
  existed,
  gone,
  opaque,
}: {
  path: string;
  before: string | null;
  after: string | null;
  beforeMedia: MediaRef | null;
  afterMedia: MediaRef | null;
  /** The file existed when the session started. */
  existed: boolean;
  /** The file is deleted now. */
  gone: boolean;
  opaque: string | null;
}) {
  const model = useModel();
  const snap = usePlayer();
  const stats = model.sessionStats(path, snap.played);
  const { dir, name } = splitPath(path);
  const oldFile = useMemo<FileContents | null>(
    () => (before === null ? null : { contents: before, name: path }),
    [path, before],
  );
  const newFile = useMemo<FileContents | null>(
    () => (after === null ? null : { contents: after, name: path }),
    [path, after],
  );
  const isImage = beforeMedia !== null || afterMedia !== null;
  const status = existed ? (gone ? 'deleted' : null) : gone ? null : 'created';
  return (
    <article className="aa-feed-edit">
      <header className="aa-feed-edit-head">
        <span className="aa-pane-path">
          <span className="dir">{`‎${dir}‎`}</span>
          <span className="name">{name}</span>
        </span>
        {status && (
          <span
            className="aa-change-tag"
            data-change={status === 'created' ? 'added' : 'removed'}
          >
            {status}
          </span>
        )}
        <Counts added={stats.added} removed={stats.removed} />
        <span className="time">
          {stats.edits} {stats.edits === 1 ? 'edit' : 'edits'}
        </span>
      </header>
      <div className="aa-diff">
        {!existed && gone ? (
          <div className="aa-diff-opaque">
            Created and deleted during the session.
          </div>
        ) : isImage ? (
          <ImageDiff after={afterMedia} before={beforeMedia} name={name} />
        ) : opaque ? (
          <div className="aa-diff-opaque">{opaque}</div>
        ) : oldFile === null && newFile === null ? (
          <div className="aa-diff-opaque">Empty file.</div>
        ) : oldFile === null ? (
          <MultiFileDiff
            newFile={newFile as FileContents}
            oldFile={null}
            options={SESSION_OPTIONS}
            style={SESSION_DIFF_STYLE}
          />
        ) : newFile === null ? (
          <MultiFileDiff
            newFile={null}
            oldFile={oldFile}
            options={SESSION_OPTIONS}
            style={SESSION_DIFF_STYLE}
          />
        ) : (
          <MultiFileDiff
            newFile={newFile}
            oldFile={oldFile}
            options={SESSION_OPTIONS}
            style={SESSION_DIFF_STYLE}
          />
        )}
      </div>
    </article>
  );
});
