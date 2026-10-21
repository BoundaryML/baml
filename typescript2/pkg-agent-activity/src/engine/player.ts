// Playback: which edits and runs are visible, and the one being shown now.
//
// Edits arrive in bursts about five seconds apart, and each one gets a short
// animation: travel to the file, write the change, then hold so it can be
// read. Runs play in the same sequence, where they happened: travel to the
// code that ran, then show what it produced. When items arrive faster than
// they can be shown, the player speeds up in proportion to the backlog and,
// past a limit, applies edits without animating until it has caught up. When
// nothing is queued the player is idle, which is where an agent spends most
// of its time.

import type { ActivityModel, Edit, Run } from './model';

export interface ActiveEdit {
  edit: Edit;
  /** `performance.now()` when the edit started. */
  startedAt: number;
  /** Camera or pane travel before the text changes. */
  travelMs: number;
  /** The text change itself. */
  writeMs: number;
  /** Hold after writing so the change can be read. */
  holdMs: number;
  /** Multiplier applied to the base durations; above 1 while catching up. */
  rate: number;
}

export interface ActiveRun {
  run: Run;
  startedAt: number;
  travelMs: number;
  /** How long the run's card stays in front before the next item plays. */
  showMs: number;
}

export type ActivePhase = 'travel' | 'write' | 'hold';

export interface PlayerSnapshot {
  /** Number of edits whose effects are visible. */
  played: number;
  /** Number of runs that are visible (runs are ordered by start). */
  runsShown: number;
  active: ActiveEdit | null;
  activeRun: ActiveRun | null;
  /** Following the live edge (false while scrubbing history). */
  following: boolean;
  paused: boolean;
  speed: number;
  /** Edits and runs received but not yet shown. */
  queued: number;
  /** Applying edits without animation to clear a backlog. */
  skipping: boolean;
}

/** Past this many queued items, apply without animation until caught up. */
const SKIP_ABOVE = 14;
const SKIP_UNTIL = 3;

export class Player {
  private snapshot: PlayerSnapshot = {
    active: null,
    activeRun: null,
    following: true,
    paused: false,
    played: 0,
    queued: 0,
    runsShown: 0,
    skipping: false,
    speed: 1,
  };
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private seenHellos = 0;
  private unsubscribe: () => void;

  constructor(private model: ActivityModel) {
    this.unsubscribe = model.subscribe(() => this.onModel());
  }

  dispose() {
    this.unsubscribe();
    if (this.timer) clearTimeout(this.timer);
  }

  getSnapshot = (): PlayerSnapshot => this.snapshot;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(patch: Partial<PlayerSnapshot>) {
    const next = { ...this.snapshot, ...patch };
    next.queued =
      Math.max(0, this.model.edits.length - next.played) +
      Math.max(0, this.model.runs.length - next.runsShown);
    this.snapshot = next;
    for (const l of this.listeners) l();
  }

  private onModel() {
    // A page that opens (or reconnects) mid-session starts at the live edge:
    // what the hello carries already happened, and replaying it would make
    // the latest files look missing until the replay caught up. Earlier
    // edits stay a scrub away on the timeline.
    if (this.model.hellos !== this.seenHellos) {
      this.seenHellos = this.model.hellos;
      if (this.timer) clearTimeout(this.timer);
      this.timer = null;
      this.set({
        active: null,
        activeRun: null,
        following: true,
        played: this.model.edits.length,
        runsShown: this.model.runs.length,
        skipping: false,
      });
      return;
    }
    // A reconnect can shrink history; never point past the end.
    const s = this.snapshot;
    if (
      s.played > this.model.edits.length ||
      s.runsShown > this.model.runs.length
    ) {
      this.set({
        active: null,
        activeRun: null,
        played: Math.min(s.played, this.model.edits.length),
        runsShown: Math.min(s.runsShown, this.model.runs.length),
      });
    } else {
      this.set({});
    }
    this.pump();
  }

  setSpeed(speed: number) {
    this.set({ speed });
  }

  setPaused(paused: boolean) {
    this.set({ paused });
    if (!paused) this.pump();
  }

  /** Jump to the state after `played` edits (and the runs before the next one); stop following. */
  seek(played: number) {
    this.cancel();
    const clamped = Math.max(0, Math.min(played, this.model.edits.length));
    const runsShown = this.model.runs.filter(
      (r) => r.afterEdit <= clamped,
    ).length;
    this.set({
      active: null,
      activeRun: null,
      following:
        clamped >= this.model.edits.length &&
        runsShown >= this.model.runs.length,
      played: clamped,
      runsShown,
    });
  }

  /** Resume following the live edge from wherever playback is. */
  follow() {
    this.set({ following: true, paused: false });
    this.pump();
  }

  /** Jump to the live edge without animating the backlog. */
  jumpToLive() {
    this.cancel();
    this.set({
      active: null,
      activeRun: null,
      following: true,
      played: this.model.edits.length,
      runsShown: this.model.runs.length,
      skipping: false,
    });
  }

  private cancel() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private pump() {
    const s = this.snapshot;
    if (this.timer || s.paused || !s.following) return;
    const edits = this.model.edits;
    const runs = this.model.runs;
    const nextRun = runs[s.runsShown];
    const runDue = nextRun !== undefined && nextRun.afterEdit <= s.played;
    if (!runDue && s.played >= edits.length) {
      if (s.active || s.activeRun || s.skipping)
        this.set({ active: null, activeRun: null, skipping: false });
      return;
    }

    const queued = edits.length - s.played + (runs.length - s.runsShown);
    const skipping = s.skipping ? queued > SKIP_UNTIL : queued > SKIP_ABOVE;
    if (skipping) {
      // Apply everything except the last few edits instantly, then animate those.
      const played = Math.max(s.played, edits.length - SKIP_UNTIL);
      const runsShown = runs.filter((r) => r.afterEdit <= played).length;
      this.set({
        active: null,
        activeRun: null,
        played,
        runsShown,
        skipping: true,
      });
      this.timer = setTimeout(() => {
        this.timer = null;
        this.set({ skipping: false });
        this.pump();
      }, 120);
      return;
    }

    const rate = s.speed * (1 + Math.max(0, queued - 1) * 0.6);
    let total: number;
    if (runDue) {
      const prev = edits[s.played - 1];
      const activeRun: ActiveRun = {
        run: nextRun,
        showMs: 2600 / rate,
        startedAt: performance.now(),
        travelMs: (prev && nextRun.touches[0] === prev.path ? 240 : 560) / rate,
      };
      this.set({ active: null, activeRun, runsShown: s.runsShown + 1 });
      total = activeRun.travelMs + activeRun.showMs;
    } else {
      const edit = edits[s.played];
      const prev = s.played > 0 ? edits[s.played - 1] : undefined;
      const active = timingFor(edit, prev, rate);
      this.set({ active, activeRun: null, played: s.played + 1 });
      total = active.travelMs + active.writeMs + active.holdMs;
    }
    this.timer = setTimeout(() => {
      this.timer = null;
      const now = this.snapshot;
      const done =
        now.played >= this.model.edits.length &&
        now.runsShown >= this.model.runs.length;
      if (done) this.set({ active: null, activeRun: null });
      this.pump();
    }, total);
  }
}

function timingFor(
  edit: Edit,
  prev: Edit | undefined,
  rate: number,
): ActiveEdit {
  const changed = Math.min(edit.added + edit.removed, 60);
  const switching = !prev || prev.path !== edit.path;
  const travel = switching ? 560 : 220;
  // About 40ms per changed line keeps most edits (86% are 30 lines or fewer)
  // under 1.5s; whole-file writes stream in over a fixed window.
  const write =
    edit.kind === 'created' ? 1500 : clamp(380 + changed * 38, 520, 2200);
  const hold = 900;
  return {
    edit,
    holdMs: hold / Math.max(1, rate * 1.5),
    rate,
    startedAt: performance.now(),
    travelMs: travel / rate,
    writeMs: write / rate,
  };
}

const clamp = (v: number, lo: number, hi: number) =>
  Math.min(hi, Math.max(lo, v));

/** Progress of the active edit at `now`: its phase and 0..1 within it. */
export function phaseAt(
  active: ActiveEdit,
  now: number,
): { phase: ActivePhase; t: number } {
  const elapsed = now - active.startedAt;
  if (elapsed < active.travelMs)
    return { phase: 'travel', t: elapsed / active.travelMs };
  const w = elapsed - active.travelMs;
  if (w < active.writeMs) return { phase: 'write', t: w / active.writeMs };
  const h = w - active.writeMs;
  return { phase: 'hold', t: Math.min(1, h / Math.max(1, active.holdMs)) };
}
