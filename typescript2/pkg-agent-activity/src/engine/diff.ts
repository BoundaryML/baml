// Line diffs for the activity model.
//
// Agent edits are small and local, so the common prefix and suffix are trimmed
// first and Myers' algorithm only runs on the region in between. A rewrite too
// large to diff cheaply degrades to one replaced block, which renders the same
// way an agent's whole-file write does.

export type LineOp =
  | { type: 'equal'; oldIndex: number; newIndex: number; text: string }
  | { type: 'delete'; oldIndex: number; text: string }
  | { type: 'insert'; newIndex: number; text: string };

export interface Hunk {
  /** 0-based first line in the old text covered by this hunk (including context). */
  oldStart: number;
  oldCount: number;
  /** 0-based first line in the new text covered by this hunk (including context). */
  newStart: number;
  newCount: number;
  ops: LineOp[];
  added: number;
  removed: number;
}

/** Upper bound on edit distance before the diff gives up and replaces the region. */
const MAX_EDIT_DISTANCE = 1500;

export function splitLines(text: string | null): string[] {
  if (text === null || text === '') return [];
  const lines = text.split('\n');
  // A trailing newline is a line terminator, not an empty last line.
  if (lines.at(-1) === '') lines.pop();
  return lines;
}

export function diffLines(a: string[], b: string[]): LineOp[] {
  let prefix = 0;
  const maxPrefix = Math.min(a.length, b.length);
  while (prefix < maxPrefix && a[prefix] === b[prefix]) prefix++;
  let suffix = 0;
  const maxSuffix = Math.min(a.length, b.length) - prefix;
  while (suffix < maxSuffix && a.at(-1 - suffix) === b.at(-1 - suffix))
    suffix++;

  const ops: LineOp[] = [];
  for (let i = 0; i < prefix; i++)
    ops.push({ newIndex: i, oldIndex: i, text: a[i], type: 'equal' });
  const middle = myers(
    a,
    prefix,
    a.length - suffix,
    b,
    prefix,
    b.length - suffix,
  );
  ops.push(...middle);
  for (let k = 0; k < suffix; k++) {
    const oi = a.length - suffix + k;
    const ni = b.length - suffix + k;
    ops.push({ newIndex: ni, oldIndex: oi, text: a[oi], type: 'equal' });
  }
  return ops;
}

/** Myers' O((N+M)D) diff on a[aLo..aHi) and b[bLo..bHi). */
function myers(
  a: string[],
  aLo: number,
  aHi: number,
  b: string[],
  bLo: number,
  bHi: number,
): LineOp[] {
  const n = aHi - aLo;
  const m = bHi - bLo;
  if (n === 0 && m === 0) return [];
  if (n === 0)
    return range(bLo, bHi).map((j) => ({
      newIndex: j,
      text: b[j],
      type: 'insert',
    }));
  if (m === 0)
    return range(aLo, aHi).map((i) => ({
      oldIndex: i,
      text: a[i],
      type: 'delete',
    }));

  const max = Math.min(n + m, MAX_EDIT_DISTANCE);
  const offset = max + 1;
  const v = new Int32Array(2 * max + 3);
  const trace: Int32Array[] = [];
  let found = false;
  outer: for (let d = 0; d <= max; d++) {
    trace.push(v.slice());
    for (let k = -d; k <= d; k += 2) {
      let x: number;
      if (k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1]))
        x = v[offset + k + 1];
      else x = v[offset + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[aLo + x] === b[bLo + y]) {
        x++;
        y++;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        found = true;
        break outer;
      }
    }
  }
  if (!found) {
    return [
      ...range(aLo, aHi).map(
        (i): LineOp => ({ oldIndex: i, text: a[i], type: 'delete' }),
      ),
      ...range(bLo, bHi).map(
        (j): LineOp => ({ newIndex: j, text: b[j], type: 'insert' }),
      ),
    ];
  }

  // Walk the trace backwards to recover the edit script.
  const ops: LineOp[] = [];
  let x = n;
  let y = m;
  for (let d = trace.length - 1; d >= 0; d--) {
    const vd = trace[d];
    const k = x - y;
    let prevK: number;
    if (k === -d || (k !== d && vd[offset + k - 1] < vd[offset + k + 1]))
      prevK = k + 1;
    else prevK = k - 1;
    const prevX = vd[offset + prevK];
    const prevY = prevX - prevK;
    while (x > prevX && y > prevY) {
      x--;
      y--;
      ops.push({
        newIndex: bLo + y,
        oldIndex: aLo + x,
        text: a[aLo + x],
        type: 'equal',
      });
    }
    if (d > 0) {
      if (x === prevX) {
        y--;
        ops.push({ newIndex: bLo + y, text: b[bLo + y], type: 'insert' });
      } else {
        x--;
        ops.push({ oldIndex: aLo + x, text: a[aLo + x], type: 'delete' });
      }
    }
  }
  ops.reverse();
  return ops;
}

function range(lo: number, hi: number): number[] {
  const out: number[] = [];
  for (let i = lo; i < hi; i++) out.push(i);
  return out;
}

/** Group an edit script into hunks with `context` unchanged lines around each change. */
export function toHunks(ops: LineOp[], context = 3): Hunk[] {
  const changed: number[] = [];
  ops.forEach((op, i) => {
    if (op.type !== 'equal') changed.push(i);
  });
  if (changed.length === 0) return [];

  const hunks: Hunk[] = [];
  let start = Math.max(0, changed[0] - context);
  let end = Math.min(ops.length, changed[0] + context + 1);
  const flush = () => {
    const slice = ops.slice(start, end);
    let oldStart = -1;
    let newStart = -1;
    let oldCount = 0;
    let newCount = 0;
    let added = 0;
    let removed = 0;
    for (const op of slice) {
      if (op.type !== 'insert') {
        if (oldStart < 0) oldStart = op.oldIndex;
        oldCount++;
      }
      if (op.type !== 'delete') {
        if (newStart < 0) newStart = op.newIndex;
        newCount++;
      }
      if (op.type === 'insert') added++;
      if (op.type === 'delete') removed++;
    }
    // A pure insertion into an empty file has no old line; anchor it at 0.
    if (oldStart < 0) oldStart = anchorOld(ops, start);
    if (newStart < 0) newStart = anchorNew(ops, start);
    hunks.push({
      added,
      newCount,
      newStart,
      oldCount,
      oldStart,
      ops: slice,
      removed,
    });
  };
  for (const idx of changed.slice(1)) {
    if (idx - context <= end) {
      end = Math.min(ops.length, idx + context + 1);
    } else {
      flush();
      start = Math.max(0, idx - context);
      end = Math.min(ops.length, idx + context + 1);
    }
  }
  flush();
  return hunks;
}

function anchorOld(ops: LineOp[], before: number): number {
  for (let i = before - 1; i >= 0; i--) {
    const op = ops[i];
    if (op.type !== 'insert') return op.oldIndex + 1;
  }
  return 0;
}

function anchorNew(ops: LineOp[], before: number): number {
  for (let i = before - 1; i >= 0; i--) {
    const op = ops[i];
    if (op.type !== 'delete') return op.newIndex + 1;
  }
  return 0;
}

/** A unified diff for one file, in the format `git diff` prints. */
export function unifiedPatch(
  path: string,
  before: string | null,
  after: string | null,
  context = 3,
): string {
  const a = splitLines(before);
  const b = splitLines(after);
  const hunks = toHunks(diffLines(a, b), context);
  const out: string[] = [];
  out.push(`diff --git a/${path} b/${path}`);
  if (before === null) out.push('new file mode 100644');
  if (after === null) out.push('deleted file mode 100644');
  out.push(before === null ? '--- /dev/null' : `--- a/${path}`);
  out.push(after === null ? '+++ /dev/null' : `+++ b/${path}`);
  for (const h of hunks) {
    const oldHeader =
      h.oldCount === 0 ? `${h.oldStart},0` : `${h.oldStart + 1},${h.oldCount}`;
    const newHeader =
      h.newCount === 0 ? `${h.newStart},0` : `${h.newStart + 1},${h.newCount}`;
    out.push(`@@ -${oldHeader} +${newHeader} @@`);
    for (const op of h.ops) {
      const sigil =
        op.type === 'equal' ? ' ' : op.type === 'insert' ? '+' : '-';
      out.push(sigil + op.text);
    }
  }
  return `${out.join('\n')}\n`;
}

export interface LineStats {
  added: number;
  removed: number;
}

export function countChanges(ops: LineOp[]): LineStats {
  let added = 0;
  let removed = 0;
  for (const op of ops) {
    if (op.type === 'insert') added++;
    else if (op.type === 'delete') removed++;
  }
  return { added, removed };
}
