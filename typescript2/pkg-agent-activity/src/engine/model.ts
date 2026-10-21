// The activity model: every file change since the session started, in order,
// with the diff and the declaration-level changes each one made.
//
// The model is append-only and indexed by edit position. Views ask for state
// "after the first n edits", which is what makes playback, scrubbing, and a
// reconnect replay all the same operation.

import {
  countChanges,
  diffLines,
  type Hunk,
  type LineOp,
  splitLines,
  toHunks,
} from './diff';
import {
  compareOutlines,
  extractOutline,
  type Language,
  languageOf,
  type OutlineSymbol,
  type SymbolChange,
} from './outline';
import type {
  AgentNote,
  ChangeKind,
  FileChange,
  GitState,
  HelloMessage,
  MediaRef,
  RunInfo,
  ServerMessage,
  SessionInfo,
  Splice,
  Workflow,
} from './protocol';

/** SVG files larger than this are shown as text only. */
const MAX_SVG_CHARS = 2_000_000;

/**
 * An SVG file's content as an image. Loaded through `<img>`, where its
 * scripts do not run and it cannot fetch anything.
 */
export function svgMedia(path: string, text: string | null): MediaRef | null {
  if (text === null || text.length > MAX_SVG_CHARS) return null;
  if (!path.toLowerCase().endsWith('.svg')) return null;
  return {
    mime: 'image/svg+xml',
    url: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`,
  };
}

/** Edits closer together than this belong to the same burst. */
export const BURST_GAP_MS = 20_000;

export interface Edit {
  index: number;
  seq: number;
  time: number;
  path: string;
  kind: ChangeKind;
  before: string | null;
  after: string | null;
  ops: LineOp[];
  hunks: Hunk[];
  added: number;
  removed: number;
  symbols: SymbolChange[];
  burst: number;
  /** Content is not shown as text (binary or over the size cap). */
  opaque: boolean;
  /** Over the server's size cap, so not even an image can be shown. */
  tooLarge: boolean;
  /** The file as an image before and after the edit, when it renders as one. */
  beforeMedia: MediaRef | null;
  afterMedia: MediaRef | null;
  /** Size in bytes after the edit, when known. */
  size: number | null;
  /** First changed line in the new text, for scrolling. */
  focusLine: number;
}

export interface Burst {
  index: number;
  /** Edit index range [start, end). */
  start: number;
  end: number;
  startTime: number;
  endTime: number;
}

export interface FileState {
  path: string;
  language: Language;
  baseline: string | null;
  /** The baseline as an image, when it renders as one. */
  baselineMedia: MediaRef | null;
  /** Indices into `edits`, ascending. */
  edits: number[];
}

/** An agent message, placed in the edit sequence where it was said. */
export interface Note extends AgentNote {
  /** Number of edits that had arrived when the note was said. */
  afterEdit: number;
}

/** A run, placed in the edit sequence where it started. */
export interface Run extends RunInfo {
  index: number;
  /** Number of edits that had arrived when the run started; it plays after them. */
  afterEdit: number;
}

export interface LedgerEntry {
  path: string;
  change: SymbolChange;
  /** Index of the most recent edit that touched this declaration. */
  lastEdit: number;
  /** Number of edits that touched it. */
  touches: number;
}

type Listener = () => void;

export class ActivityModel {
  session: SessionInfo | null = null;
  git: GitState | null = null;
  readonly edits: Edit[] = [];
  readonly bursts: Burst[] = [];
  readonly files = new Map<string, FileState>();
  readonly runs: Run[] = [];
  private runById = new Map<string, Run>();
  readonly notes: Note[] = [];
  /** Every `flows` message, in arrival order, placed after the edits that preceded it. */
  private flowHistory: {
    afterEdit: number;
    project: string;
    workflows: Workflow[];
  }[] = [];
  /** Bumped on every mutation; lets React memoize on a number. */
  version = 0;
  /**
   * Bumped by every `hello`. The history a hello carries has already
   * happened, so the player shows it at once instead of replaying it.
   */
  hellos = 0;

  private listeners = new Set<Listener>();
  private outlines = new Map<number, OutlineSymbol[]>();
  private baselineOutlines = new Map<string, OutlineSymbol[]>();

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit() {
    this.version++;
    for (const l of this.listeners) l();
  }

  apply(message: ServerMessage) {
    switch (message.type) {
      case 'hello':
        this.reset(message);
        break;
      case 'change':
        if (
          message.baseline !== undefined &&
          !this.files.has(message.change.path)
        ) {
          this.fileState(
            message.change.path,
            message.baseline,
            message.baselineMedia,
          );
        }
        this.append(message.change);
        break;
      case 'git':
        this.git = message.git;
        break;
      case 'run':
        this.upsertRun(message.run, this.edits.length);
        break;
      case 'note':
        this.notes.push({ ...message.note, afterEdit: this.edits.length });
        break;
      case 'flows':
        this.flowHistory.push({
          afterEdit: this.edits.length,
          project: message.project,
          workflows: message.workflows,
        });
        break;
    }
    this.emit();
  }

  private upsertRun(info: RunInfo, afterEdit: number) {
    const existing = this.runById.get(info.id);
    if (existing) {
      Object.assign(existing, info);
      return;
    }
    const run: Run = { ...info, afterEdit, index: this.runs.length };
    this.runs.push(run);
    this.runById.set(run.id, run);
  }

  private reset(hello: HelloMessage) {
    this.hellos++;
    this.session = hello.session;
    this.git = hello.session.git;
    this.edits.length = 0;
    this.bursts.length = 0;
    this.files.clear();
    this.outlines.clear();
    this.baselineOutlines.clear();
    this.statsCache.clear();
    this.runs.length = 0;
    this.runById.clear();
    for (const [path, baseline] of Object.entries(hello.baselines))
      this.fileState(path, baseline, hello.baselineMedia?.[path]);
    for (const change of hello.changes) this.append(change);
    for (const run of hello.runs ?? []) {
      const afterEdit = this.edits.filter(
        (e) => e.time <= run.startedAt,
      ).length;
      this.upsertRun(run, afterEdit);
    }
    this.notes.length = 0;
    for (const note of hello.notes ?? []) {
      this.notes.push({
        ...note,
        afterEdit: this.edits.filter((e) => e.time <= note.time).length,
      });
    }
    // A reconnect only gets the latest graphs, so scrubbing back shows them too.
    this.flowHistory = (hello.flows ?? []).map((f) => ({
      afterEdit: 0,
      project: f.project,
      workflows: f.workflows,
    }));
  }

  /** The workflows of every project as of the first `played` edits. */
  workflowsAt(played: number): Workflow[] {
    const latest = new Map<string, Workflow[]>();
    for (const f of this.flowHistory) {
      if (f.afterEdit > played) break;
      latest.set(f.project, f.workflows);
    }
    return [...latest.values()].flat();
  }

  /** The last thing the agent said before edit `index` arrived: that edit's caption. */
  noteBefore(index: number): Note | undefined {
    let found: Note | undefined;
    for (const n of this.notes) {
      if (n.afterEdit > index) break;
      found = n;
    }
    return found;
  }

  /** Captions of the first `played` edits to `path`, oldest first, each once, with the edits it covers. */
  notesForFile(
    path: string,
    played: number,
  ): { note: Note; edits: number[] }[] {
    const file = this.files.get(path);
    if (!file) return [];
    const out: { note: Note; edits: number[] }[] = [];
    for (const index of file.edits) {
      if (index >= played) break;
      const note = this.noteBefore(index);
      if (!note) continue;
      const last = out.at(-1);
      if (last && last.note === note) last.edits.push(index);
      else out.push({ edits: [index], note });
    }
    return out;
  }

  /** Notes said before the edit after `played` (the caption for that moment is the last one). */
  notesAt(played: number): Note[] {
    let end = 0;
    while (end < this.notes.length && this.notes[end].afterEdit <= played)
      end++;
    return this.notes.slice(0, end);
  }

  private fileState(
    path: string,
    baseline: string | null,
    baselineMedia?: MediaRef,
  ): FileState {
    let file = this.files.get(path);
    if (!file) {
      file = {
        baseline,
        baselineMedia: baselineMedia ?? svgMedia(path, baseline),
        edits: [],
        language: languageOf(path),
        path,
      };
      this.files.set(path, file);
    }
    return file;
  }

  private append(change: FileChange) {
    const file = this.fileState(change.path, null);
    const index = this.edits.length;
    const before = this.contentAt(change.path, index);
    const after =
      change.after !== undefined
        ? change.after
        : change.splices
          ? applySplices(before, change.splices)
          : before;
    const opaque = Boolean(change.binary || change.tooLarge);
    const beforeMedia = this.mediaAt(change.path, index);
    const afterMedia = change.media ?? svgMedia(change.path, after);
    const a = splitLines(before);
    const b = splitLines(after);
    const ops = opaque ? [] : diffLines(a, b);
    const { added, removed } = countChanges(ops);
    const hunks = toHunks(ops, 3);

    const touchedNew = new Set<number>();
    const touchedOld = new Set<number>();
    for (const op of ops) {
      if (op.type === 'insert') touchedNew.add(op.newIndex);
      if (op.type === 'delete') touchedOld.add(op.oldIndex);
    }
    const outlineBefore = this.outlineAt(change.path, index);
    const outlineAfter = opaque
      ? outlineBefore
      : extractOutline(change.path, after);
    this.outlines.set(index, outlineAfter);
    const symbols = opaque
      ? []
      : compareOutlines(outlineBefore, outlineAfter, touchedNew, touchedOld);

    const prev = this.edits[index - 1];
    let burst = this.bursts.at(-1);
    if (!prev || !burst || change.time - prev.time > BURST_GAP_MS) {
      burst = {
        end: index,
        endTime: change.time,
        index: this.bursts.length,
        start: index,
        startTime: change.time,
      };
      this.bursts.push(burst);
    }
    burst.end = index + 1;
    burst.endTime = change.time;

    const firstInsert = ops.find((op) => op.type === 'insert');
    const firstDelete = ops.find((op) => op.type === 'delete');
    let focusLine = 0;
    if (firstInsert && firstInsert.type === 'insert')
      focusLine = firstInsert.newIndex;
    else if (firstDelete) {
      const idx = ops.indexOf(firstDelete);
      for (let j = idx - 1; j >= 0; j--) {
        const op = ops[j];
        if (op.type !== 'delete') {
          focusLine = op.newIndex + 1;
          break;
        }
      }
    }

    this.edits.push({
      added,
      after,
      afterMedia,
      before,
      beforeMedia,
      burst: burst.index,
      focusLine,
      hunks,
      index,
      kind: change.kind,
      opaque,
      ops,
      path: change.path,
      removed,
      seq: change.seq,
      size: change.size,
      symbols,
      time: change.time,
      tooLarge: Boolean(change.tooLarge),
    });
    file.edits.push(index);
  }

  /** Content of `path` after the first `count` edits. */
  contentAt(path: string, count: number): string | null {
    const file = this.files.get(path);
    if (!file) return null;
    const last = lastBelow(file.edits, count);
    return last === undefined ? file.baseline : this.edits[last].after;
  }

  /** `path` as an image after the first `count` edits, when it renders as one. */
  mediaAt(path: string, count: number): MediaRef | null {
    const file = this.files.get(path);
    if (!file) return null;
    const last = lastBelow(file.edits, count);
    return last === undefined
      ? file.baselineMedia
      : this.edits[last].afterMedia;
  }

  /** Whether `path` existed when the session started. */
  existedAtStart(path: string): boolean {
    const file = this.files.get(path);
    if (!file) return false;
    const first = file.edits[0];
    return first === undefined
      ? file.baseline !== null || file.baselineMedia !== null
      : this.edits[first].kind !== 'created';
  }

  /** Whether `path` is absent after the first `count` edits. */
  deletedAt(path: string, count: number): boolean {
    const last = this.lastEditOf(path, count);
    return last === undefined
      ? !this.existedAtStart(path)
      : this.edits[last].kind === 'deleted';
  }

  /** Index of the last edit to `path` among the first `count` edits. */
  lastEditOf(path: string, count: number): number | undefined {
    const file = this.files.get(path);
    return file ? lastBelow(file.edits, count) : undefined;
  }

  outlineAt(path: string, count: number): OutlineSymbol[] {
    const last = this.lastEditOf(path, count);
    if (last !== undefined) return this.outlines.get(last) ?? [];
    let cached = this.baselineOutlines.get(path);
    if (!cached) {
      cached = extractOutline(path, this.files.get(path)?.baseline ?? null);
      this.baselineOutlines.set(path, cached);
    }
    return cached;
  }

  baselineOutline(path: string): OutlineSymbol[] {
    return this.outlineAt(path, 0);
  }

  /** Files touched by the first `count` edits, in first-touch order. */
  touchedFiles(count: number): FileState[] {
    const seen = new Set<string>();
    const out: FileState[] = [];
    for (let i = 0; i < Math.min(count, this.edits.length); i++) {
      const path = this.edits[i].path;
      if (seen.has(path)) continue;
      seen.add(path);
      const f = this.files.get(path);
      if (f) out.push(f);
    }
    return out;
  }

  /** Lines added and removed relative to the baseline, per file, after `count` edits. */
  sessionStats(
    path: string,
    count: number,
  ): { added: number; removed: number; edits: number } {
    const file = this.files.get(path);
    if (!file) return { added: 0, edits: 0, removed: 0 };
    const edits = file.edits.filter((i) => i < count).length;
    const current = this.contentAt(path, count);
    const key = `${path}\u0000${count}`;
    const cached = this.statsCache.get(key);
    if (cached) return { ...cached, edits };
    const ops = diffLines(splitLines(file.baseline), splitLines(current));
    const stats = countChanges(ops);
    this.statsCache.set(key, stats);
    return { ...stats, edits };
  }
  private statsCache = new Map<string, { added: number; removed: number }>();

  /**
   * Declarations that differ between the session baseline and the state after
   * `count` edits, with the edit that last touched each one. Body-only changes
   * are included when an edit touched the declaration's lines.
   */
  ledger(count: number): LedgerEntry[] {
    const touches = new Map<string, { lastEdit: number; touches: number }>();
    for (let i = 0; i < Math.min(count, this.edits.length); i++) {
      const edit = this.edits[i];
      for (const s of edit.symbols) {
        const id = `${edit.path}\u0000${s.key}`;
        const t = touches.get(id) ?? { lastEdit: i, touches: 0 };
        t.lastEdit = i;
        t.touches++;
        touches.set(id, t);
      }
    }
    const out: LedgerEntry[] = [];
    for (const file of this.touchedFiles(count)) {
      const before = this.baselineOutline(file.path);
      const after = this.outlineAt(file.path, count);
      const changes = compareOutlines(before, after, new Set(), new Set());
      const reported = new Set<string>();
      for (const change of changes) {
        const t = touches.get(`${file.path}\u0000${change.key}`);
        out.push({
          change,
          lastEdit: t?.lastEdit ?? -1,
          path: file.path,
          touches: t?.touches ?? 0,
        });
        reported.add(change.key);
      }
      // Declarations whose body changed but whose surface did not.
      for (const sym of after) {
        if (reported.has(sym.key)) continue;
        const t = touches.get(`${file.path}\u0000${sym.key}`);
        if (!t) continue;
        const prev = before.find((s) => s.key === sym.key);
        out.push({
          change: {
            after: sym,
            before: prev,
            change: 'body',
            container: sym.container,
            key: sym.key,
            kind: sym.kind,
            members: [],
            name: sym.name,
          },
          lastEdit: t.lastEdit,
          path: file.path,
          touches: t.touches,
        });
      }
    }
    return out;
  }
}

export function applySplices(text: string | null, splices: Splice[]): string {
  const lines = (text ?? '').split('\n');
  for (const s of splices) lines.splice(s.at, s.remove, ...s.insert);
  return lines.join('\n');
}

function lastBelow(sorted: number[], limit: number): number | undefined {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (sorted[mid] < limit) lo = mid + 1;
    else hi = mid;
  }
  return lo === 0 ? undefined : sorted[lo - 1];
}
