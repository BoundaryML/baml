import { describe, expect, it } from 'vitest';

import {
  NO_NOT_READY_PROJECTS,
  applyProjectUpdateToGating,
  isRunGated,
  markProjectNotReady,
} from './run-gating';
import {
  PROJECT_NOT_READY_ERROR_CODE,
  RunCommandError,
  isProjectNotReadyError,
} from './run-store-client';

describe('run gating (fail-closed playground server)', () => {
  it('gates runs while the latest update reports a stale engine', () => {
    expect(
      isRunGated(NO_NOT_READY_PROJECTS, '/project', { isBexCurrent: false }),
    ).toBe(true);
    expect(
      isRunGated(NO_NOT_READY_PROJECTS, '/project', { isBexCurrent: true }),
    ).toBe(false);
  });

  it('does not gate while no project is selected or no update arrived yet', () => {
    expect(isRunGated(NO_NOT_READY_PROJECTS, null, undefined)).toBe(false);
    expect(isRunGated(NO_NOT_READY_PROJECTS, '/project', undefined)).toBe(
      false,
    );
  });

  it('keeps a projectNotReady rejection gating until a current update arrives', () => {
    let state = markProjectNotReady(NO_NOT_READY_PROJECTS, '/project');

    // Even if the last seen update claimed the engine was current, the
    // server's rejection wins until the NEXT current update arrives.
    expect(isRunGated(state, '/project', { isBexCurrent: true })).toBe(true);
    expect(isRunGated(state, '/other', { isBexCurrent: true })).toBe(false);

    // A stale update keeps the mark…
    state = applyProjectUpdateToGating(state, '/project', {
      isBexCurrent: false,
    });
    expect(isRunGated(state, '/project', { isBexCurrent: false })).toBe(true);

    // …and a current one clears it, re-enabling runs automatically.
    state = applyProjectUpdateToGating(state, '/project', {
      isBexCurrent: true,
    });
    expect(isRunGated(state, '/project', { isBexCurrent: true })).toBe(false);
  });

  it('scopes updates to their own project', () => {
    let state = markProjectNotReady(NO_NOT_READY_PROJECTS, '/a');
    state = markProjectNotReady(state, '/b');

    state = applyProjectUpdateToGating(state, '/a', { isBexCurrent: true });

    expect(isRunGated(state, '/a', { isBexCurrent: true })).toBe(false);
    expect(isRunGated(state, '/b', { isBexCurrent: true })).toBe(true);
  });

  it('reuses state objects when nothing changes', () => {
    const marked = markProjectNotReady(NO_NOT_READY_PROJECTS, '/project');
    expect(markProjectNotReady(marked, '/project')).toBe(marked);
    expect(
      applyProjectUpdateToGating(marked, '/project', { isBexCurrent: false }),
    ).toBe(marked);
    expect(
      applyProjectUpdateToGating(marked, '/other', { isBexCurrent: true }),
    ).toBe(marked);
  });

  it('recognizes projectNotReady command errors by code, not message text', () => {
    const notReady = new RunCommandError(
      PROJECT_NOT_READY_ERROR_CODE,
      'Cannot start run: rebuild pending',
    );
    expect(isProjectNotReadyError(notReady)).toBe(true);
    expect(notReady.message).toBe(
      'projectNotReady: Cannot start run: rebuild pending',
    );

    expect(
      isProjectNotReadyError(new RunCommandError('invalidArguments', 'nope')),
    ).toBe(false);
    expect(
      isProjectNotReadyError(new Error('projectNotReady: message-only fake')),
    ).toBe(false);
    expect(isProjectNotReadyError(undefined)).toBe(false);
  });
});
