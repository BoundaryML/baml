import type { ReactNode } from 'react';
import { useModel, usePlayer, useRuntime, useTicker } from '../runtime';
import { formatDuration, splitPath } from './symbols';

export type ViewId =
  | 'stage'
  | 'tiles'
  | 'flow'
  | 'graph'
  | 'contracts'
  | 'feed';

export const VIEWS: { id: ViewId; label: string; hint: string }[] = [
  {
    hint: 'Every touched file on a canvas; the camera follows the agent',
    id: 'stage',
    label: 'Stage',
  },
  {
    hint: 'The files being edited, tiled like terminal panes',
    id: 'tiles',
    label: 'Tiles',
  },
  {
    hint: 'The program as a workflow of steps; edits light up the steps they touch',
    id: 'flow',
    label: 'Flow',
  },
  {
    hint: 'Declarations and their references; edits fly into the node they change',
    id: 'graph',
    label: 'Graph',
  },
  {
    hint: 'The types and signatures that changed since the session started',
    id: 'contracts',
    label: 'Contracts',
  },
  { hint: 'Every edit as a diff, grouped by burst', id: 'feed', label: 'Feed' },
];

const SPEEDS = [0.5, 1, 2, 4];

interface MenuBarProps {
  view: ViewId;
  onView: (view: ViewId) => void;
  captions: boolean;
  onCaptions: (on: boolean) => void;
  filesOpen: boolean;
  onToggleFiles: () => void;
}

export function MenuBar({
  view,
  onView,
  captions,
  onCaptions,
  filesOpen,
  onToggleFiles,
}: MenuBarProps) {
  const { player, source, status } = useRuntime();
  const model = useModel();
  const snap = usePlayer();
  useTicker(500);

  const git = model.git;
  const session = model.session;
  const offline = source.kind === 'live' && status !== 'open';
  const writing = snap.active !== null;
  const lastEdit = model.edits[snap.played - 1];
  const idleMs = lastEdit ? source.now() - lastEdit.time : 0;

  let state: 'writing' | 'idle' | 'offline' =
    writing || snap.activeRun ? 'writing' : 'idle';
  if (offline) state = 'offline';

  let statusText: ReactNode;
  if (offline) {
    statusText =
      status === 'connecting'
        ? 'Connecting to baml agent watch…'
        : 'Disconnected; retrying';
  } else if (snap.active) {
    statusText = (
      <>
        Writing <strong>{splitPath(snap.active.edit.path).name}</strong>
      </>
    );
  } else if (snap.activeRun) {
    statusText = (
      <>
        Ran{' '}
        <strong>
          {snap.activeRun.run.target?.name ?? snap.activeRun.run.command}
        </strong>
      </>
    );
  } else if (!snap.following) {
    statusText = `Viewing edit ${snap.played} of ${model.edits.length}`;
  } else if (!lastEdit) {
    statusText = 'Waiting for the first edit';
  } else {
    statusText = (
      <>
        No edits for <strong>{formatDuration(idleMs)}</strong>
        {source.fastForwarding() && (
          <span
            className="aa-fastforward"
            title="Replaying: idle time is compressed"
          >
            {fastForwardIcon}
          </span>
        )}
      </>
    );
  }

  return (
    <header className="aa-menubar">
      <div className="aa-project">
        <button
          aria-label={
            filesOpen ? 'Hide the files panel' : 'Show the files panel'
          }
          aria-pressed={filesOpen}
          className="aa-icon-button"
          onClick={onToggleFiles}
          title={filesOpen ? 'Hide files (B)' : 'Show files (B)'}
          type="button"
        >
          {sidebarIcon}
        </button>
        <span aria-hidden="true" className="aa-presence" data-state={state} />
        <span className="aa-project-name">
          {session?.name ?? 'Agent activity'}
        </span>
        {git && (
          <>
            <span
              className="aa-branch"
              title={`HEAD ${git.head}: ${git.headSubject}`}
            >
              {branchIcon}
              {git.branch ?? 'detached'}
            </span>
            <span className="aa-sha">{git.head.slice(0, 7)}</span>
            {git.commits.length > 0 && (
              <span
                className="aa-commits"
                title={git.commits
                  .map((c) => `${c.sha.slice(0, 7)} ${c.subject}`)
                  .join('\n')}
              >
                {git.commits.length}{' '}
                {git.commits.length === 1 ? 'commit' : 'commits'} this session
              </span>
            )}
          </>
        )}
      </div>

      <nav aria-label="View" className="aa-segmented">
        {VIEWS.map((v, i) => (
          <button
            aria-pressed={view === v.id}
            key={v.id}
            onClick={() => onView(v.id)}
            title={v.hint}
            type="button"
          >
            {v.label}
            <kbd>{i + 1}</kbd>
          </button>
        ))}
      </nav>

      <div className="aa-controls">
        <div className="aa-status" data-state={state}>
          {statusText}
        </div>
        {snap.queued > 0 && snap.following && (
          <span className="aa-queue">{snap.queued} queued</span>
        )}
        {!snap.following && (
          <button
            className="aa-live-button"
            onClick={() => player.follow()}
            type="button"
          >
            Resume live
          </button>
        )}
        <fieldset
          aria-label="Playback speed"
          className="aa-segmented aa-segmented--small"
        >
          {SPEEDS.map((s) => (
            <button
              aria-pressed={snap.speed === s}
              key={s}
              onClick={() => {
                player.setSpeed(s);
                source.setSpeed(s);
              }}
              type="button"
            >
              {s === 0.5 ? '½×' : `${s}×`}
            </button>
          ))}
        </fieldset>
        <button
          aria-label="Show the agent's messages as captions"
          aria-pressed={captions}
          className="aa-icon-button"
          onClick={() => onCaptions(!captions)}
          title={
            model.notes.length
              ? "Captions: the agent's messages from its transcript"
              : 'Captions: no agent transcript found for this session'
          }
          type="button"
        >
          {captionsIcon}
        </button>
        <button
          aria-label={snap.paused ? 'Resume playback' : 'Pause playback'}
          className="aa-icon-button"
          onClick={() => player.setPaused(!snap.paused)}
          title={snap.paused ? 'Resume (space)' : 'Pause (space)'}
          type="button"
        >
          {snap.paused ? playIcon : pauseIcon}
        </button>
      </div>
    </header>
  );
}

const branchIcon = (
  <svg
    aria-hidden="true"
    fill="none"
    height="12"
    viewBox="0 0 16 16"
    width="12"
  >
    <circle cx="4" cy="3.5" r="1.75" stroke="currentColor" strokeWidth="1.5" />
    <circle cx="4" cy="12.5" r="1.75" stroke="currentColor" strokeWidth="1.5" />
    <circle cx="12" cy="5" r="1.75" stroke="currentColor" strokeWidth="1.5" />
    <path
      d="M4 5.25v5.5M12 6.75c0 3-4.5 2.5-7.2 4.4"
      stroke="currentColor"
      strokeWidth="1.5"
    />
  </svg>
);

const pauseIcon = (
  <svg aria-hidden="true" height="14" viewBox="0 0 16 16" width="14">
    <rect fill="currentColor" height="11" rx="1" width="3" x="3.5" y="2.5" />
    <rect fill="currentColor" height="11" rx="1" width="3" x="9.5" y="2.5" />
  </svg>
);

const playIcon = (
  <svg aria-hidden="true" height="14" viewBox="0 0 16 16" width="14">
    <path
      d="M4 2.8v10.4a.8.8 0 0 0 1.2.7l8.6-5.2a.8.8 0 0 0 0-1.4L5.2 2.1A.8.8 0 0 0 4 2.8Z"
      fill="currentColor"
    />
  </svg>
);

const sidebarIcon = (
  <svg
    aria-hidden="true"
    fill="none"
    height="16"
    viewBox="0 0 16 16"
    width="16"
  >
    <rect
      height="11"
      rx="2"
      stroke="currentColor"
      strokeWidth="1.4"
      width="13"
      x="1.5"
      y="2.5"
    />
    <path d="M6 2.5v11" stroke="currentColor" strokeWidth="1.4" />
  </svg>
);

const captionsIcon = (
  <svg
    aria-hidden="true"
    fill="none"
    height="16"
    viewBox="0 0 16 16"
    width="16"
  >
    <rect
      height="10"
      rx="2"
      stroke="currentColor"
      strokeWidth="1.4"
      width="13"
      x="1.5"
      y="3"
    />
    <path
      d="M4.5 8.5h3M9 8.5h2.5M4.5 10.8h7"
      stroke="currentColor"
      strokeLinecap="round"
      strokeWidth="1.4"
    />
  </svg>
);

const fastForwardIcon = (
  <svg aria-hidden="true" height="12" viewBox="0 0 16 16" width="12">
    <path d="M1.5 3.5v9l6-4.5zM8.5 3.5v9l6-4.5z" fill="currentColor" />
  </svg>
);
