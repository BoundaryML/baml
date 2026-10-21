import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { diffLines, splitLines, toHunks, unifiedPatch } from './diff';
import { ActivityModel } from './model';
import { compareOutlines, extractOutline } from './outline';
import { Player } from './player';
import type { ServerMessage } from './protocol';

describe('diffLines', () => {
  it('finds a replaced line between shared context', () => {
    const ops = diffLines(['a', 'b', 'c'], ['a', 'x', 'c']);
    expect(ops.map((op) => op.type)).toEqual([
      'equal',
      'delete',
      'insert',
      'equal',
    ]);
  });

  it('treats a trailing newline as a terminator', () => {
    expect(splitLines('one\ntwo\n')).toEqual(['one', 'two']);
    expect(splitLines('')).toEqual([]);
  });

  it('groups changes into hunks with context', () => {
    const a = Array.from({ length: 20 }, (_, i) => `line ${i}`);
    const b = [...a];
    b[2] = 'changed near the top';
    b[17] = 'changed near the bottom';
    const hunks = toHunks(diffLines(a, b), 2);
    expect(hunks).toHaveLength(2);
    expect(hunks[0]).toMatchObject({ added: 1, oldStart: 0, removed: 1 });
  });

  it('prints a unified patch for a new file', () => {
    expect(unifiedPatch('a.txt', null, 'x\ny\n')).toBe(
      'diff --git a/a.txt b/a.txt\nnew file mode 100644\n--- /dev/null\n+++ b/a.txt\n@@ -0,0 +1,2 @@\n+x\n+y\n',
    );
  });
});

describe('extractOutline', () => {
  it('reads BAML classes with fields, functions, and quoted test names', () => {
    const outline = extractOutline(
      'main.baml',
      [
        'class Resume {',
        '    name: string,',
        '    skills: string[] @description("{ braces in a string }"),',
        '}',
        '',
        'function Extract(text: string) -> Resume {',
        '    client: Fast',
        '    prompt: `Read ${text} { not a block }`',
        '}',
        '',
        'test "extracts a resume" {',
        '    Extract("x")',
        '}',
      ].join('\n'),
    );
    expect(outline.map((s) => `${s.kind}:${s.name}`)).toEqual([
      'class:Resume',
      'function:Extract',
      'test:extracts a resume',
    ]);
    expect(outline[0].members.map((m) => m.name)).toEqual(['name', 'skills']);
    expect(outline[1]).toMatchObject({ end: 8, start: 5 });
  });

  it('reads Rust impl methods under their type', () => {
    const outline = extractOutline(
      'lib.rs',
      [
        'pub struct A {',
        '    pub x: u32,',
        '}',
        '',
        'impl A {',
        '    pub fn get(&self) -> u32 {',
        '        self.x',
        '    }',
        '}',
      ].join('\n'),
    );
    expect(outline.map((s) => s.key)).toEqual([
      'struct:A',
      'impl:A',
      'A.method:get',
    ]);
  });

  it('reads TypeScript declarations', () => {
    const outline = extractOutline(
      'a.ts',
      [
        'export interface P {',
        '  id: string;',
        '}',
        '',
        'export const load = async (id: string) => {',
        '  return id;',
        '};',
      ].join('\n'),
    );
    expect(outline.map((s) => `${s.kind}:${s.name}`)).toEqual([
      'interface:P',
      'function:load',
    ]);
  });

  it('reports added, changed, and removed declarations', () => {
    const before = extractOutline(
      'a.baml',
      'class A {\n  x: int,\n}\n\nfunction F(a: int) -> int {\n  a\n}\n',
    );
    const after = extractOutline(
      'a.baml',
      'class A {\n  x: int,\n  y: string,\n}\n\nfunction G() -> int {\n  1\n}\n',
    );
    const changes = compareOutlines(before, after, new Set(), new Set());
    expect(changes.map((c) => `${c.change}:${c.name}`).sort()).toEqual([
      'added:G',
      'members:A',
      'removed:F',
    ]);
    expect(changes.find((c) => c.name === 'A')?.members).toEqual([
      { after: 'y: string', kind: 'added', name: 'y' },
    ]);
  });
});

function hello(): ServerMessage {
  return {
    baselines: {},
    changes: [],
    protocol: 1,
    session: { git: null, name: 't', root: '/t', startedAt: 0 },
    type: 'hello',
  };
}

describe('ActivityModel', () => {
  it('applies full contents and splices, and derives the previous content', () => {
    const model = new ActivityModel();
    model.apply(hello());
    model.apply({
      baseline: 'a\nb\nc',
      change: {
        after: 'a\nB\nc',
        kind: 'modified',
        path: 'f.txt',
        seq: 1,
        size: 5,
        time: 10,
      },
      type: 'change',
    });
    model.apply({
      change: {
        kind: 'modified',
        path: 'f.txt',
        seq: 2,
        size: null,
        splices: [{ at: 3, insert: ['d'], remove: 0 }],
        time: 20,
      },
      type: 'change',
    });
    expect(model.contentAt('f.txt', 0)).toBe('a\nb\nc');
    expect(model.contentAt('f.txt', 1)).toBe('a\nB\nc');
    expect(model.contentAt('f.txt', 2)).toBe('a\nB\nc\nd');
    expect(model.edits[1].before).toBe('a\nB\nc');
    expect(model.sessionStats('f.txt', 2)).toEqual({
      added: 2,
      edits: 2,
      removed: 1,
    });
  });

  it('places runs, notes, and workflows after the edits that preceded them', () => {
    const model = new ActivityModel();
    model.apply(hello());
    model.apply({
      note: { id: 'n1', text: 'First the types.', time: 1 },
      type: 'note',
    });
    model.apply({
      baseline: null,
      change: {
        after: 'x',
        kind: 'created',
        path: 'a.baml',
        seq: 1,
        size: 1,
        time: 2,
      },
      type: 'change',
    });
    model.apply({
      project: '.',
      type: 'flows',
      workflows: [{ graph: {}, name: 'W', path: 'a.baml' }],
    });
    model.apply({
      run: {
        command: 'baml test',
        id: 'r1',
        kind: 'baml-test',
        startedAt: 3,
        status: 'running',
        touches: [],
      },
      type: 'run',
    });
    model.apply({
      run: {
        command: 'baml test',
        endedAt: 4,
        id: 'r1',
        kind: 'baml-test',
        startedAt: 3,
        status: 'passed',
        touches: [],
      },
      type: 'run',
    });
    expect(model.runs).toHaveLength(1);
    expect(model.runs[0]).toMatchObject({ afterEdit: 1, status: 'passed' });
    expect(model.noteBefore(0)?.text).toBe('First the types.');
    expect(model.workflowsAt(0)).toEqual([]);
    expect(model.workflowsAt(1).map((w) => w.name)).toEqual(['W']);
  });
});

describe('ActivityModel images', () => {
  it('tracks images across edits, from the baseline on', () => {
    const png = (id: string) => ({
      mime: 'image/png',
      url: `/api/agent-activity/media/${id}`,
    });
    const model = new ActivityModel();
    model.apply({
      ...(hello() as Extract<ServerMessage, { type: 'hello' }>),
      baselineMedia: { 'logo.png': png('a') },
      baselines: { 'logo.png': null },
      changes: [
        {
          after: null,
          binary: true,
          kind: 'modified',
          media: png('b'),
          path: 'logo.png',
          seq: 1,
          size: 10,
          time: 1,
        },
      ],
    });
    model.apply({
      change: {
        after: '<svg xmlns="http://www.w3.org/2000/svg"/>',
        kind: 'created',
        path: 'icons/dot.svg',
        seq: 2,
        size: 40,
        time: 2,
      },
      type: 'change',
    });
    const [logo, dot] = model.edits;
    expect(logo.beforeMedia?.url).toMatch(/\/a$/);
    expect(logo.afterMedia?.url).toMatch(/\/b$/);
    expect(model.mediaAt('logo.png', 0)?.url).toMatch(/\/a$/);
    expect(model.mediaAt('logo.png', 1)?.url).toMatch(/\/b$/);
    expect(dot.beforeMedia).toBeNull();
    expect(dot.afterMedia?.url).toMatch(/^data:image\/svg\+xml/);
  });
});

describe('Player', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('plays edits and runs in the order they happened', () => {
    const model = new ActivityModel();
    const player = new Player(model);
    model.apply(hello());
    model.apply({
      baseline: null,
      change: {
        after: 'x',
        kind: 'created',
        path: 'a',
        seq: 1,
        size: 1,
        time: 1,
      },
      type: 'change',
    });
    model.apply({
      run: {
        command: 'c',
        id: 'r',
        kind: 'command',
        startedAt: 2,
        status: 'passed',
        touches: [],
      },
      type: 'run',
    });
    model.apply({
      change: {
        after: 'y',
        kind: 'modified',
        path: 'a',
        seq: 2,
        size: 1,
        time: 3,
      },
      type: 'change',
    });

    const order: string[] = [];
    const record = () => {
      const s = player.getSnapshot();
      const label = s.active
        ? `edit ${s.active.edit.seq}`
        : s.activeRun
          ? `run ${s.activeRun.run.id}`
          : null;
      if (label && order.at(-1) !== label) order.push(label);
    };
    player.subscribe(record);
    record();
    vi.advanceTimersByTime(30_000);
    expect(order).toEqual(['edit 1', 'run r', 'edit 2']);
    expect(player.getSnapshot()).toMatchObject({
      active: null,
      played: 2,
      queued: 0,
      runsShown: 1,
    });
    player.dispose();
  });

  it('opens at the live edge when the hello carries history', () => {
    const model = new ActivityModel();
    const player = new Player(model);
    model.apply({
      ...(hello() as Extract<ServerMessage, { type: 'hello' }>),
      baselines: { a: null },
      changes: [
        { after: '1', kind: 'created', path: 'a', seq: 1, size: 1, time: 1 },
        { after: '2', kind: 'modified', path: 'a', seq: 2, size: 1, time: 2 },
      ],
    });
    expect(player.getSnapshot()).toMatchObject({
      active: null,
      following: true,
      played: 2,
      queued: 0,
    });
    // What arrives afterwards animates as usual.
    model.apply({
      change: {
        after: '3',
        kind: 'modified',
        path: 'a',
        seq: 3,
        size: 1,
        time: 3,
      },
      type: 'change',
    });
    expect(player.getSnapshot().active?.edit.seq).toBe(3);
    player.dispose();
  });

  it('seeking shows the state after an edit and stops following', () => {
    const model = new ActivityModel();
    const player = new Player(model);
    model.apply(hello());
    for (let i = 1; i <= 3; i++) {
      model.apply({
        change: {
          after: `${i}`,
          kind: 'modified',
          path: 'a',
          seq: i,
          size: 1,
          time: i,
        },
        type: 'change',
      });
    }
    player.seek(1);
    expect(player.getSnapshot()).toMatchObject({ following: false, played: 1 });
    player.dispose();
  });
});
