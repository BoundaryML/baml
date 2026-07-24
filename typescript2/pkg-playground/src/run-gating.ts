/**
 * Run gating for the fail-closed playground server.
 *
 * The server refuses `startRun`/`startPreviewRun`/`startTestRun` with a
 * `projectNotReady` command error while a rebuild is pending or the project
 * has compile errors, and every `ProjectUpdate` carries `isBexCurrent`.
 * The UI treats both signals as one transient "Preparing current build…"
 * state instead of surfacing raw errors:
 *
 *  - `isBexCurrent === false` on the latest update → runs are gated.
 *  - a run rejected with `projectNotReady` → the project is marked not-ready
 *    until the next `updateProject` with `isBexCurrent === true` arrives,
 *    which re-enables run controls automatically.
 *
 * Pure data helpers so the transitions are unit-testable outside React.
 */

import type { ProjectUpdate } from './worker-protocol';

/** Projects whose last run attempt was refused with `projectNotReady`. */
export type NotReadyProjects = ReadonlySet<string>;

export const NO_NOT_READY_PROJECTS: NotReadyProjects = new Set<string>();

/** Record a `projectNotReady` rejection for `project`. */
export function markProjectNotReady(
  state: NotReadyProjects,
  project: string,
): NotReadyProjects {
  if (state.has(project)) return state;
  const next = new Set(state);
  next.add(project);
  return next;
}

/**
 * Fold an `updateProject` notification into the not-ready set. A current
 * build clears the pending rejection; a stale build keeps it (the derived
 * `isBexCurrent === false` gate covers that case anyway).
 */
export function applyProjectUpdateToGating(
  state: NotReadyProjects,
  project: string,
  update: Pick<ProjectUpdate, 'isBexCurrent'>,
): NotReadyProjects {
  if (!update.isBexCurrent || !state.has(project)) return state;
  const next = new Set(state);
  next.delete(project);
  return next;
}

/**
 * Whether Run/test-run/preview actions for `project` must stay disabled while
 * the server prepares the current build. `update` is the latest ProjectUpdate
 * for the project (undefined while the project is still loading).
 */
export function isRunGated(
  state: NotReadyProjects,
  project: string | null | undefined,
  update: Pick<ProjectUpdate, 'isBexCurrent'> | undefined,
): boolean {
  if (project == null) return false;
  if (state.has(project)) return true;
  return update != null && !update.isBexCurrent;
}
