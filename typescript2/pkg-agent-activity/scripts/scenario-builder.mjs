// Builds a recorded session: the exact messages `baml agent watch` sends
// (see src/engine/protocol.ts), each with a time offset. Every scenario, from
// git history or scripted by hand, is written through this builder, so they
// all share one format and replay through the same code path as a live
// session.
//
// Times inside messages are milliseconds since the session started; the
// replaying source adds its own epoch.

export class ScenarioBuilder {
  constructor({ id, title, description, project, branch, startHead }) {
    this.meta = { description, id, title };
    this.project = project;
    this.branch = branch;
    this.startHead = startHead;
    this.t = 0;
    this.seq = 0;
    this.contents = new Map();
    this.baselines = new Map();
    this.touched = new Set();
    this.commits = [];
    this.runs = new Map();
    this.events = [];
    this.head = startHead;
  }

  /** Content a file had when the session started. */
  baseline(path, content) {
    this.contents.set(path, content);
    this.baselines.set(path, content);
    return this;
  }

  /** Advance the clock by `ms`. */
  wait(ms) {
    this.t += Math.round(ms);
    return this;
  }

  /** Set the clock to `ms` since the start (never moves backwards). */
  at(ms) {
    this.t = Math.max(this.t, Math.round(ms));
    return this;
  }

  has(path) {
    return this.contents.has(path);
  }

  content(path) {
    return this.contents.get(path) ?? null;
  }

  /** Write a whole file (create or overwrite). */
  write(path, content) {
    const existed = this.contents.has(path);
    this.contents.set(path, content);
    return this.change(path, existed ? 'modified' : 'created', {
      after: content,
    });
  }

  /** Apply line splices to an existing file. */
  splice(path, splices) {
    const lines = (this.contents.get(path) ?? '').split('\n');
    for (const s of splices) lines.splice(s.at, s.remove, ...s.insert);
    this.contents.set(path, lines.join('\n'));
    return this.change(path, 'modified', { splices });
  }

  /** Replace the first occurrence of `find` with `replace`, like an editor tool's Edit call. */
  edit(path, find, replace) {
    const before = this.contents.get(path);
    if (before === undefined) throw new Error(`edit: ${path} does not exist`);
    const at = before.indexOf(find);
    if (at < 0) throw new Error(`edit: text not found in ${path}:\n${find}`);
    const after =
      before.slice(0, at) + replace + before.slice(at + find.length);
    return this.splice(path, [lineSplice(before, after)]);
  }

  delete(path) {
    this.contents.delete(path);
    return this.change(path, 'deleted', { after: null });
  }

  change(path, kind, body) {
    const message = {
      change: { kind, path, seq: ++this.seq, time: this.t, ...body },
      type: 'change',
    };
    if (!this.touched.has(path)) {
      message.baseline = this.baselines.get(path) ?? null;
      this.touched.add(path);
    }
    if (body.after !== undefined && body.after !== null)
      message.change.size = body.after.length;
    else message.change.size = null;
    this.events.push({ message, t: this.t });
    return this;
  }

  /** Something the agent said (a caption). */
  note(text, agent = 'claude-code') {
    const note = {
      agent,
      id: `note-${this.events.length}`,
      text,
      time: this.t,
    };
    this.events.push({ message: { note, type: 'note' }, t: this.t });
    return this;
  }

  commit(sha, subject) {
    this.commits.push({ sha, subject, time: this.t });
    this.head = { sha, subject };
    this.events.push({
      message: { git: this.gitState(), type: 'git' },
      t: this.t,
    });
    return this;
  }

  /** Start a run; returns its id. Finish it with `finish(id, ...)`. */
  start(run) {
    const id = run.id ?? `run-${this.runs.size + 1}`;
    const info = {
      status: 'running',
      touches: [],
      ...run,
      id,
      startedAt: this.t,
    };
    this.runs.set(id, info);
    this.events.push({ message: { run: { ...info }, type: 'run' }, t: this.t });
    return id;
  }

  finish(id, result) {
    const info = { ...this.runs.get(id), ...result, endedAt: this.t };
    this.runs.set(id, info);
    this.events.push({ message: { run: { ...info }, type: 'run' }, t: this.t });
    return this;
  }

  gitState() {
    return {
      branch: this.branch,
      commits: this.commits.map((c) => ({ ...c })),
      head: this.head.sha,
      headSubject: this.head.subject,
      startHead: this.startHead.sha,
    };
  }

  build() {
    const hello = {
      baselines: {},
      changes: [],
      notes: [],
      protocol: 1,
      runs: [],
      session: {
        git: {
          branch: this.branch,
          commits: [],
          head: this.startHead.sha,
          headSubject: this.startHead.subject,
          startHead: this.startHead.sha,
        },
        name: this.project,
        root: `~/projects/${this.project}`,
        startedAt: 0,
      },
      type: 'hello',
    };
    return { ...this.meta, events: [{ message: hello, t: 0 }, ...this.events] };
  }
}

/** The single line splice that turns `before` into `after` (common lines trimmed). */
export function lineSplice(before, after) {
  const a = before.split('\n');
  const b = after.split('\n');
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  return { at: start, insert: b.slice(start, endB), remove: endA - start };
}

/** Small deterministic PRNG so scenarios are stable across regenerations. */
export function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
