import { describe, expect, it } from 'vitest';

import {
  ProjectPayloadFencer,
  acceptMonotonicEpoch,
  normalizeProjectCatalog,
  runtimeIsReady,
  runtimeStatusFromUpdate,
} from './project-runtime-state';

describe('project runtime catalog fencing', () => {
  it('rejects payloads from another runtime session', () => {
    const qualified = new ProjectPayloadFencer(7);
    expect(qualified.acceptSession(7)).toBe(true);
    expect(qualified.acceptSession(6)).toBe(false);
    expect(qualified.acceptSession()).toBe(false);

    const legacy = new ProjectPayloadFencer();
    expect(legacy.acceptSession()).toBe(true);
    expect(legacy.acceptSession(7)).toBe(false);
  });

  it('uses qualified entries while preserving the legacy path catalog', () => {
    expect(
      normalizeProjectCatalog(
        ['/z', '/a', '/a'],
        [
          { project: '/a', incarnation: 3, sourceRevision: 9 },
          { project: '/ignored', incarnation: 1, sourceRevision: 1 },
        ],
      ),
    ).toEqual([
      { project: '/a', incarnation: 3, sourceRevision: 9 },
      { project: '/z' },
    ]);
  });

  it('rejects stale source revisions and wrong project incarnations', () => {
    const fencer = new ProjectPayloadFencer();
    fencer.applyCatalog(['/project'], [
      { project: '/project', incarnation: 4, sourceRevision: 12 },
    ]);

    expect(fencer.accept('/project', 4, 11)).toBe(false);
    expect(fencer.accept('/project', 3, 12)).toBe(false);
    expect(fencer.accept('/project', undefined, 12)).toBe(false);
    expect(fencer.accept('/project', 4, undefined)).toBe(false);
    expect(fencer.accept('/project', 4, 12)).toBe(true);
    expect(fencer.accept('/project', 4, 13)).toBe(true);
    expect(fencer.accept('/project', 4, 12)).toBe(false);
  });

  it('does not regress catalog identity or source watermarks', () => {
    const fencer = new ProjectPayloadFencer();
    fencer.applyCatalog(['/project'], [
      { project: '/project', incarnation: 4, sourceRevision: 12 },
    ]);

    expect(
      fencer.applyCatalog(['/project'], [
        { project: '/project', incarnation: 4, sourceRevision: 11 },
      ]).entries,
    ).toEqual([
      { project: '/project', incarnation: 4, sourceRevision: 12 },
    ]);
    expect(
      fencer.applyCatalog(['/project'], [
        { project: '/project', incarnation: 3, sourceRevision: 99 },
      ]).entries,
    ).toEqual([
      { project: '/project', incarnation: 4, sourceRevision: 12 },
    ]);
  });

  it('purges remove/re-add state and rejects late payloads from the old incarnation', () => {
    const fencer = new ProjectPayloadFencer();
    fencer.applyCatalog(['/project'], [
      { project: '/project', incarnation: 1, sourceRevision: 5 },
    ]);
    expect(fencer.accept('/project', 1, 5)).toBe(true);

    const removed = fencer.applyCatalog([], []);
    expect(removed.purgedProjects).toEqual(new Set(['/project']));
    expect(fencer.accept('/project', 1, 5)).toBe(false);

    fencer.applyCatalog(['/project'], [
      { project: '/project', incarnation: 2, sourceRevision: 1 },
    ]);
    expect(fencer.accept('/project', 1, 5)).toBe(false);
    expect(fencer.accept('/project', 2, 1)).toBe(true);
  });

  it('keeps legacy WASM catalogs and payloads compatible', () => {
    const fencer = new ProjectPayloadFencer();
    fencer.applyCatalog(['/workspace']);
    expect(fencer.accept('/workspace')).toBe(true);
  });
});

describe('project runtime compatibility state', () => {
  it('rejects same-revision collection epoch regressions', () => {
    const epochs = new Map<string, number>();
    expect(acceptMonotonicEpoch(epochs, 'project/1/5', 3)).toBe(true);
    expect(acceptMonotonicEpoch(epochs, 'project/1/5', 3)).toBe(true);
    expect(acceptMonotonicEpoch(epochs, 'project/1/5', 2)).toBe(false);
    expect(acceptMonotonicEpoch(epochs, 'project/1/6', 1)).toBe(true);
  });

  it('derives a ready state from an old isBexCurrent payload', () => {
    const state = runtimeStatusFromUpdate({
      isBexCurrent: true,
      functions: [],
      diagnostics: [],
    });
    expect(state.state).toBe('ready');
    expect(runtimeIsReady(state)).toBe(true);
  });

  it('does not allow an invalid legacy payload to run its last-known-good build', () => {
    const state = runtimeStatusFromUpdate({
      isBexCurrent: false,
      functions: [
        { name: 'OldFunction', kind: 'expr', origin: 'userDefined' },
      ],
      diagnostics: [{ severity: 'error', message: 'invalid source' }],
    });
    expect(state).toMatchObject({
      state: 'blockedByDiagnostics',
      hasLastKnownGood: true,
    });
    expect(runtimeIsReady(state)).toBe(false);
  });
});
