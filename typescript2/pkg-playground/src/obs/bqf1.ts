/**
 * §9.3 BQF1: fixed little-endian columnar frame decoder.
 *
 * Mirrors the frozen Rust encoder in `crates/bex_query/src/bqf1.rs`:
 *
 * ```text
 * [40 B header]
 *   magic        4B  "BQF1"
 *   kind         u16 @4
 *   flags        u16 @6   (bit0 lod_degraded, bit1 partial_tail, bit2 more_lanes)
 *   request_id   u64 @8
 *   epoch        u64 @16
 *   ncols        u32 @24
 *   nrows        u32 @28
 *   payload_len  u64 @32  (directory + columns, excluding trailer)
 * [column directory] ncols x 16 B @40: (col_type u8, pad[7], offset u64
 *   relative to PAYLOAD START = byte 40 + ncols*16)
 * [column payloads]  each 8-byte aligned
 *   1 u32: nrows x 4 B    2 u64: nrows x 8 B    3 f64: nrows x 8 B
 *   4 str: (nrows+1) x u32 offsets, pad8, utf8 bytes
 * [8 B trailer] crc32c of everything before it, stored as u64 LE
 * ```
 *
 * Numeric columns decode to zero-copy TypedArray views over the frame's
 * ArrayBuffer (payload start = 40 + ncols*16 is 8-aligned, and per-column
 * offsets are 8-aligned, so views are always aligned when the frame starts
 * at buffer offset 0 — which a WebSocket binary message guarantees).
 * Strings decode eagerly to string[].
 */

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const BQF1_HEADER_LEN = 40;

export const FLAG_LOD_DEGRADED = 1 << 0;
export const FLAG_PARTIAL_TAIL = 1 << 1;
export const FLAG_MORE_LANES = 1 << 2;

/** Frame kinds (u16 in the header). */
export const FrameKind = {
  BqlTable: 9,
  LeftHeavy: 4,
  LiveTotals: 7,
  RecentCalls: 8,
  RunMeta: 2,
  RunsList: 1,
  Status: 6,
  Timeline: 3,
  TopFunctions: 5,
} as const;
export type FrameKind = (typeof FrameKind)[keyof typeof FrameKind];

/** LeftHeavy `function` sentinel: a synthetic "N smaller" fold row. */
export const FOLD_ROW_FUNCTION = 0xffff_ffff;

// ---------------------------------------------------------------------------
// crc32c (Castagnoli, reflected, poly 0x82F63B78) — matches the Rust
// implementation in bex_events::prof::cct::crc32c.
// ---------------------------------------------------------------------------

const CRC32C_TABLE: Uint32Array = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let crc = i;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = crc & 1 ? (crc >>> 1) ^ 0x82f63b78 : crc >>> 1;
    }
    table[i] = crc >>> 0;
  }
  return table;
})();

/** One-shot CRC32C (init 0xFFFFFFFF, final xor 0xFFFFFFFF). */
export function crc32c(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i += 1) {
    crc = (crc >>> 8) ^ CRC32C_TABLE[(crc ^ bytes[i]!) & 0xff]!;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

// ---------------------------------------------------------------------------
// Decoded frame
// ---------------------------------------------------------------------------

export type BqfColumn =
  | { type: 'u32'; data: Uint32Array }
  | { type: 'u64'; data: BigUint64Array }
  | { type: 'f64'; data: Float64Array }
  | { type: 'str'; data: string[] };

export interface BqfFrame {
  kind: number;
  flags: number;
  /** request_id echoed from the query/sub JSON `id` (u64; ids are small). */
  requestId: number;
  /** Data generation the frame was computed at. */
  epoch: number;
  nrows: number;
  cols: BqfColumn[];
}

export class BqfDecodeError extends Error {}

const utf8 = new TextDecoder('utf-8');

/**
 * Decode one BQF1 frame. Throws `BqfDecodeError` on truncation, bad magic,
 * CRC mismatch, or a malformed directory.
 */
export function decodeFrame(buf: ArrayBuffer): BqfFrame {
  const bytes = new Uint8Array(buf);
  if (bytes.length < BQF1_HEADER_LEN + 8) {
    throw new BqfDecodeError('BQF1 frame too short');
  }
  // magic "BQF1"
  if (
    bytes[0] !== 0x42 ||
    bytes[1] !== 0x51 ||
    bytes[2] !== 0x46 ||
    bytes[3] !== 0x31
  ) {
    throw new BqfDecodeError('BQF1 bad magic');
  }

  const view = new DataView(buf);
  const body = bytes.subarray(0, bytes.length - 8);
  const storedCrc = view.getBigUint64(bytes.length - 8, true);
  if (BigInt(crc32c(body)) !== storedCrc) {
    throw new BqfDecodeError('BQF1 CRC mismatch');
  }

  const kind = view.getUint16(4, true);
  const flags = view.getUint16(6, true);
  const requestId = Number(view.getBigUint64(8, true));
  const epoch = Number(view.getBigUint64(16, true));
  const ncols = view.getUint32(24, true);
  const nrows = view.getUint32(28, true);
  const payloadLen = Number(view.getBigUint64(32, true));

  const dirEnd = BQF1_HEADER_LEN + ncols * 16;
  if (dirEnd > body.length || BQF1_HEADER_LEN + payloadLen > body.length) {
    throw new BqfDecodeError('BQF1 bad directory');
  }
  const payloadStart = dirEnd; // absolute byte offset of payload start
  const payloadEnd = BQF1_HEADER_LEN + payloadLen;

  const colOffset = (i: number): number =>
    Number(view.getBigUint64(BQF1_HEADER_LEN + i * 16 + 8, true));

  const cols: BqfColumn[] = [];
  for (let i = 0; i < ncols; i += 1) {
    const colType = bytes[BQF1_HEADER_LEN + i * 16]!;
    const off = colOffset(i);
    const at = payloadStart + off;
    // Column extent = next column's offset (or payload end); readers take
    // nrows-bounded prefixes, mirroring the Rust decoder.
    const end = i + 1 < ncols ? payloadStart + colOffset(i + 1) : payloadEnd;
    if (at > payloadEnd || end > payloadEnd || at > end) {
      throw new BqfDecodeError('BQF1 bad column offset');
    }
    switch (colType) {
      case 1: {
        if (at + nrows * 4 > end)
          throw new BqfDecodeError('BQF1 u32 column truncated');
        cols.push({ data: new Uint32Array(buf, at, nrows), type: 'u32' });
        break;
      }
      case 2: {
        if (at + nrows * 8 > end)
          throw new BqfDecodeError('BQF1 u64 column truncated');
        cols.push({ data: new BigUint64Array(buf, at, nrows), type: 'u64' });
        break;
      }
      case 3: {
        if (at + nrows * 8 > end)
          throw new BqfDecodeError('BQF1 f64 column truncated');
        cols.push({ data: new Float64Array(buf, at, nrows), type: 'f64' });
        break;
      }
      case 4: {
        cols.push({ data: decodeStrColumn(buf, at, end, nrows), type: 'str' });
        break;
      }
      default:
        throw new BqfDecodeError(`BQF1 unknown column type ${colType}`);
    }
  }

  return { cols, epoch, flags, kind, nrows, requestId };
}

function decodeStrColumn(
  buf: ArrayBuffer,
  at: number,
  end: number,
  nrows: number,
): string[] {
  const offsetsLen = (nrows + 1) * 4;
  if (at + offsetsLen > end) {
    throw new BqfDecodeError('BQF1 str offsets truncated');
  }
  // `at` is 8-aligned, so a u32 view is aligned.
  const offsets = new Uint32Array(buf, at, nrows + 1);
  // The utf8 blob starts at the 8-aligned boundary after the offsets.
  let blobAt = at + offsetsLen;
  blobAt += (8 - (blobAt % 8)) % 8;
  const blob = new Uint8Array(buf, blobAt, end - blobAt);
  const out: string[] = [];
  for (let i = 0; i < nrows; i += 1) {
    const from = offsets[i]!;
    const to = offsets[i + 1]!;
    if (from > to || to > blob.length) {
      throw new BqfDecodeError('BQF1 str extent out of range');
    }
    out.push(from === to ? '' : utf8.decode(blob.subarray(from, to)));
  }
  return out;
}

// ---------------------------------------------------------------------------
// Typed per-kind helpers
// ---------------------------------------------------------------------------

interface ColDataByType {
  u32: Uint32Array;
  u64: BigUint64Array;
  f64: Float64Array;
  str: string[];
}

function expectCol<T extends keyof ColDataByType>(
  frame: BqfFrame,
  index: number,
  type: T,
  kindName: string,
): ColDataByType[T] {
  const col = frame.cols[index];
  if (!col || col.type !== type) {
    throw new BqfDecodeError(
      `BQF1 ${kindName}: expected col ${index} of type ${type}, got ${col?.type ?? 'missing'}`,
    );
  }
  return col.data as ColDataByType[T];
}

function expectKind(frame: BqfFrame, kind: FrameKind, kindName: string): void {
  if (frame.kind !== kind) {
    throw new BqfDecodeError(
      `BQF1: expected ${kindName} (kind ${kind}), got kind ${frame.kind}`,
    );
  }
}

/** Convert a u64 column to JS numbers (values fit: ns within runs, ms epochs). */
export function u64ToNumbers(data: BigUint64Array): Float64Array {
  const out = new Float64Array(data.length);
  for (let i = 0; i < data.length; i += 1) out[i] = Number(data[i]!);
  return out;
}

export interface RunsListColumns {
  runKey: string[];
  boundaryId: string[];
  target: string[];
  source: string[];
  status: string[];
  revision: string[];
  createdMs: Float64Array;
  completedMs: Float64Array;
  hasSnapshot: Uint32Array;
}

export function asRunsList(frame: BqfFrame): RunsListColumns {
  expectKind(frame, FrameKind.RunsList, 'RunsList');
  return {
    boundaryId: expectCol(frame, 1, 'str', 'RunsList'),
    completedMs: u64ToNumbers(expectCol(frame, 7, 'u64', 'RunsList')),
    createdMs: u64ToNumbers(expectCol(frame, 6, 'u64', 'RunsList')),
    hasSnapshot: expectCol(frame, 8, 'u32', 'RunsList'),
    revision: expectCol(frame, 5, 'str', 'RunsList'),
    runKey: expectCol(frame, 0, 'str', 'RunsList'),
    source: expectCol(frame, 3, 'str', 'RunsList'),
    status: expectCol(frame, 4, 'str', 'RunsList'),
    target: expectCol(frame, 2, 'str', 'RunsList'),
  };
}

export interface RunMetaColumns {
  functionId: Uint32Array;
  fqn: string[];
}

export function asRunMeta(frame: BqfFrame): RunMetaColumns {
  expectKind(frame, FrameKind.RunMeta, 'RunMeta');
  return {
    fqn: expectCol(frame, 1, 'str', 'RunMeta'),
    functionId: expectCol(frame, 0, 'u32', 'RunMeta'),
  };
}

export interface TimelineColumns {
  /** Raw u64 thread ids (may exceed 2^53; use the raw view for identity). */
  thread: BigUint64Array;
  firstTsNs: Float64Array;
  lastTsNs: Float64Array;
  busyNs: Float64Array;
  awaitNs: Float64Array;
  dominantFunction: Uint32Array;
  errors: Float64Array;
}

export function asTimeline(frame: BqfFrame): TimelineColumns {
  expectKind(frame, FrameKind.Timeline, 'Timeline');
  return {
    awaitNs: u64ToNumbers(expectCol(frame, 4, 'u64', 'Timeline')),
    busyNs: u64ToNumbers(expectCol(frame, 3, 'u64', 'Timeline')),
    dominantFunction: expectCol(frame, 5, 'u32', 'Timeline'),
    errors: u64ToNumbers(expectCol(frame, 6, 'u64', 'Timeline')),
    firstTsNs: u64ToNumbers(expectCol(frame, 1, 'u64', 'Timeline')),
    lastTsNs: u64ToNumbers(expectCol(frame, 2, 'u64', 'Timeline')),
    thread: expectCol(frame, 0, 'u64', 'Timeline'),
  };
}

export interface LeftHeavyColumns {
  depth: Uint32Array;
  /** 0xFFFFFFFF (`FOLD_ROW_FUNCTION`) marks a synthetic "smaller" fold row. */
  functionId: Uint32Array;
  totalNs: Float64Array;
  selfNs: Float64Array;
  enters: Float64Array;
  errors: Float64Array;
  foldedCount: Uint32Array;
}

export function asLeftHeavy(frame: BqfFrame): LeftHeavyColumns {
  expectKind(frame, FrameKind.LeftHeavy, 'LeftHeavy');
  return {
    depth: expectCol(frame, 0, 'u32', 'LeftHeavy'),
    enters: u64ToNumbers(expectCol(frame, 4, 'u64', 'LeftHeavy')),
    errors: u64ToNumbers(expectCol(frame, 5, 'u64', 'LeftHeavy')),
    foldedCount: expectCol(frame, 6, 'u32', 'LeftHeavy'),
    functionId: expectCol(frame, 1, 'u32', 'LeftHeavy'),
    selfNs: u64ToNumbers(expectCol(frame, 3, 'u64', 'LeftHeavy')),
    totalNs: u64ToNumbers(expectCol(frame, 2, 'u64', 'LeftHeavy')),
  };
}

export interface TopFunctionsColumns {
  functionId: Uint32Array;
  calls: Float64Array;
  totalNs: Float64Array;
  selfNs: Float64Array;
  errors: Float64Array;
}

export function asTopFunctions(frame: BqfFrame): TopFunctionsColumns {
  expectKind(frame, FrameKind.TopFunctions, 'TopFunctions');
  return {
    calls: u64ToNumbers(expectCol(frame, 1, 'u64', 'TopFunctions')),
    errors: u64ToNumbers(expectCol(frame, 4, 'u64', 'TopFunctions')),
    functionId: expectCol(frame, 0, 'u32', 'TopFunctions'),
    selfNs: u64ToNumbers(expectCol(frame, 3, 'u64', 'TopFunctions')),
    totalNs: u64ToNumbers(expectCol(frame, 2, 'u64', 'TopFunctions')),
  };
}

export interface RecentCallsColumns {
  /** Raw u64 `(partition << 32) | thread_idx` ids; use as identity only. */
  thread: BigUint64Array;
  /** Per-thread call ids; unique key is `(thread, callId)`. */
  callId: BigUint64Array;
  parentCallId: BigUint64Array;
  functionId: Uint32Array;
  /**
   * Raw u64 nanosecond timestamps. Kept as BigUint64Array so callers can
   * subtract a baseline in BigInt space before converting to Number (epoch-ns
   * values exceed 2^53; deltas within a run do not).
   */
  startNs: BigUint64Array;
  endNs: BigUint64Array;
  /** FunctionEndStatus: 0 ok, 1 errored, 2 cancelled, 3 exited. */
  status: Uint32Array;
}

/** §9.4 exact-recency tier: completed calls from the live engine's rings. */
export function asRecentCalls(frame: BqfFrame): RecentCallsColumns {
  expectKind(frame, FrameKind.RecentCalls, 'RecentCalls');
  return {
    callId: expectCol(frame, 1, 'u64', 'RecentCalls'),
    endNs: expectCol(frame, 5, 'u64', 'RecentCalls'),
    functionId: expectCol(frame, 3, 'u32', 'RecentCalls'),
    parentCallId: expectCol(frame, 2, 'u64', 'RecentCalls'),
    startNs: expectCol(frame, 4, 'u64', 'RecentCalls'),
    status: expectCol(frame, 6, 'u32', 'RecentCalls'),
    thread: expectCol(frame, 0, 'u64', 'RecentCalls'),
  };
}

export interface StatusColumns {
  code: Uint32Array;
  message: string[];
}

/**
 * Generic BQL result (kind 9): free-form data columns + a final Str meta
 * column. Frame row 0 is a meta row (sentinels; the meta col's row 0 holds
 * `{"columns":[{name,type}...],"rows":N,"footer":{...}}`); data row i is
 * frame row i+1 — so an empty result still ships its footer.
 */
export interface BqlTableResult {
  columns: Array<{ name: string; values: Array<string | number> }>;
  rows: number;
  footer: {
    sealed: boolean;
    torn: boolean;
    degraded: string[];
  };
}

export function asBqlTable(frame: BqfFrame): BqlTableResult {
  expectKind(frame, FrameKind.BqlTable, 'BqlTable');
  const metaCol = frame.cols[frame.cols.length - 1];
  if (!metaCol || metaCol.type !== 'str') {
    throw new BqfDecodeError('BQF1 BqlTable: missing meta column');
  }
  const metaRaw = metaCol.data[0] ?? '';
  let meta: {
    columns?: Array<{ name?: string; type?: string }>;
    rows?: number;
    footer?: { sealed?: boolean; torn?: boolean; degraded?: string[] };
  };
  try {
    meta = JSON.parse(metaRaw) as typeof meta;
  } catch {
    throw new BqfDecodeError('BQF1 BqlTable: bad meta JSON');
  }
  const described = meta.columns ?? [];
  const dataRows = Math.max(0, frame.nrows - 1);
  const columns: BqlTableResult['columns'] = [];
  for (let i = 0; i < described.length && i < frame.cols.length - 1; i += 1) {
    const col = frame.cols[i]!;
    const values: Array<string | number> = [];
    for (let row = 1; row <= dataRows; row += 1) {
      switch (col.type) {
        case 'u32':
          values.push(col.data[row] ?? 0);
          break;
        case 'u64':
          values.push(Number(col.data[row] ?? 0n));
          break;
        case 'f64':
          values.push(col.data[row] ?? 0);
          break;
        case 'str':
          values.push(col.data[row] ?? '');
          break;
      }
    }
    columns.push({ name: described[i]?.name ?? `col${i}`, values });
  }
  return {
    columns,
    footer: {
      degraded: meta.footer?.degraded ?? [],
      sealed: meta.footer?.sealed ?? true,
      torn: meta.footer?.torn ?? false,
    },
    rows: dataRows,
  };
}

export function asStatus(frame: BqfFrame): StatusColumns {
  expectKind(frame, FrameKind.Status, 'Status');
  return {
    code: expectCol(frame, 0, 'u32', 'Status'),
    message: expectCol(frame, 1, 'str', 'Status'),
  };
}
