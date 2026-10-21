// Replays a recorded session: the same messages `baml agent watch` sends,
// each with its time offset. The recording's own clock drives the session
// time; stretches where the agent was thinking are fast-forwarded so a
// half-hour session plays in a few minutes, while the timeline still shows
// the real rhythm.

import type { ServerMessage } from '../engine/protocol';
import type { ActivitySource, ConnectionStatus } from './source';

export interface RecordedEvent {
  /** Milliseconds since the session started. */
  t: number;
  /** Times inside the message are also milliseconds since the session started. */
  message: ServerMessage;
}

export interface RecordedSession {
  id: string;
  title: string;
  description: string;
  events: RecordedEvent[];
}

export interface MockScenarioInfo {
  id: string;
  title: string;
  /** One line shown on the picker button. */
  summary: string;
  description: string;
  load: () => Promise<RecordedSession>;
}

export const MOCK_SCENARIOS: MockScenarioInfo[] = [
  {
    description:
      'A scripted session covering every event kind: small files in a row, a burst of generated files, edits, deletes, BAML runs with output, a failing test and its fix, and captions.',
    id: 'resume-screener',
    load: () =>
      import('../mock/fixtures/resume-screener.json').then(
        (m) => m.default as RecordedSession,
      ),
    summary: 'Creates, deletes, runs, and a failing test',
    title: 'Build a resume screener',
  },
  {
    description:
      'Standard-library BAML, a reflection change in the VM, new BAML tests, and a blog post, replayed from commit 05a266fc7.',
    id: 'typesafe-ai',
    load: () =>
      import('../mock/fixtures/typesafe-ai.json').then(
        (m) => m.default as RecordedSession,
      ),
    summary: '38 edits to BAML, Rust, and docs',
    title: 'Add a typesafe AI client',
  },
  {
    description:
      'A BAML project and its React front end written from scratch, replayed from commit da9b950f4.',
    id: 'type-quiz',
    load: () =>
      import('../mock/fixtures/type-quiz.json').then(
        (m) => m.default as RecordedSession,
      ),
    summary: '29 edits creating 15 files',
    title: 'Build the type quiz',
  },
  {
    description:
      'Five commits refining one Rust module and its protocol tests, replayed from this branch.',
    id: 'inlay-hints',
    load: () =>
      import('../mock/fixtures/inlay-hints.json').then(
        (m) => m.default as RecordedSession,
      ),
    summary: '38 edits, test runs, 5 commits',
    title: 'Quiet the inlay hints',
  },
];

/** Gaps up to this long play in real time; the rest is compressed. */
const REAL_TIME_GAP_MS = 3500;
const IDLE_COMPRESSION = 0.05;

interface Segment {
  wallStart: number;
  wallDuration: number;
  virtualStart: number;
  virtualDuration: number;
}

export class MockSource implements ActivitySource {
  readonly kind = 'mock' as const;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private speed = 1;
  private epoch = Date.now();
  private segment: Segment = {
    virtualDuration: 0,
    virtualStart: 0,
    wallDuration: 1,
    wallStart: performance.now(),
  };
  private next = 0;
  private emit: ((m: ServerMessage) => void) | null = null;

  constructor(private session: RecordedSession) {}

  start(
    onMessage: (message: ServerMessage) => void,
    onStatus: (status: ConnectionStatus) => void,
  ) {
    this.emit = onMessage;
    this.epoch = Date.now();
    this.next = 0;
    this.segment = {
      virtualDuration: 0,
      virtualStart: 0,
      wallDuration: 1,
      wallStart: performance.now(),
    };
    onStatus('open');
    this.schedule();
  }

  stop() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.emit = null;
  }

  setSpeed(speed: number) {
    // Keep the virtual clock continuous across the change.
    const now = performance.now();
    const virtualNow = this.virtualElapsed(now);
    const remainingVirtual =
      this.segment.virtualStart + this.segment.virtualDuration - virtualNow;
    this.speed = speed;
    if (this.timer && remainingVirtual > 0) {
      clearTimeout(this.timer);
      this.timer = null;
      this.startSegment(virtualNow, remainingVirtual, now);
    }
  }

  now() {
    return this.epoch + this.virtualElapsed(performance.now());
  }

  fastForwarding() {
    const s = this.segment;
    return (
      this.timer !== null &&
      s.virtualDuration > s.wallDuration * 1.5 * this.speed
    );
  }

  private virtualElapsed(wallNow: number): number {
    const s = this.segment;
    const f = Math.min(
      1,
      Math.max(0, (wallNow - s.wallStart) / Math.max(1, s.wallDuration)),
    );
    return s.virtualStart + f * s.virtualDuration;
  }

  private schedule() {
    const event = this.session.events[this.next];
    if (!event) return;
    const now = performance.now();
    const virtualNow = this.virtualElapsed(now);
    this.startSegment(virtualNow, Math.max(0, event.t - virtualNow), now);
  }

  private startSegment(
    virtualStart: number,
    virtualDuration: number,
    wallNow: number,
  ) {
    const compressed =
      virtualDuration <= REAL_TIME_GAP_MS
        ? virtualDuration
        : REAL_TIME_GAP_MS +
          (virtualDuration - REAL_TIME_GAP_MS) * IDLE_COMPRESSION;
    const wallDuration = compressed / this.speed;
    this.segment = {
      virtualDuration,
      virtualStart,
      wallDuration,
      wallStart: wallNow,
    };
    this.timer = setTimeout(() => {
      this.timer = null;
      this.fire();
    }, wallDuration);
  }

  private fire() {
    // Deliver every event due at this instant, then wait for the next.
    const due = this.session.events[this.next]?.t;
    while (
      this.emit &&
      this.next < this.session.events.length &&
      this.session.events[this.next].t === due
    ) {
      this.emit(rebase(this.session.events[this.next++].message, this.epoch));
    }
    this.schedule();
  }
}

/** Shift a recorded message's session-relative times onto the wall clock. */
function rebase(message: ServerMessage, epoch: number): ServerMessage {
  const at = (t: number) => epoch + t;
  const git = <T extends { commits: { time: number }[] } | null>(g: T): T =>
    g
      ? { ...g, commits: g.commits.map((c) => ({ ...c, time: at(c.time) })) }
      : g;
  switch (message.type) {
    case 'hello':
      return {
        ...message,
        changes: message.changes.map((c) => ({ ...c, time: at(c.time) })),
        notes: message.notes?.map((n) => ({ ...n, time: at(n.time) })),
        runs: message.runs?.map((r) => ({
          ...r,
          endedAt: r.endedAt && at(r.endedAt),
          startedAt: at(r.startedAt),
        })),
        session: {
          ...message.session,
          git: git(message.session.git),
          startedAt: at(message.session.startedAt),
        },
      };
    case 'change':
      return {
        ...message,
        change: { ...message.change, time: at(message.change.time) },
      };
    case 'git':
      return { ...message, git: git(message.git) };
    case 'run':
      return {
        ...message,
        run: {
          ...message.run,
          endedAt:
            message.run.endedAt === undefined
              ? undefined
              : at(message.run.endedAt),
          startedAt: at(message.run.startedAt),
        },
      };
    case 'note':
      return {
        ...message,
        note: { ...message.note, time: at(message.note.time) },
      };
    case 'flows':
      return message;
  }
}
