import { describe, expect, it } from 'vitest';

import {
  asLeftHeavy,
  asRunsList,
  asStatus,
  BqfDecodeError,
  crc32c,
  decodeFrame,
  FOLD_ROW_FUNCTION,
  FrameKind,
} from './bqf1';

// ---------------------------------------------------------------------------
// Test-only BQF1 encoder, mirroring the frozen wire spec exactly
// (crates/bex_query/src/bqf1.rs::encode_frame).
// ---------------------------------------------------------------------------

type EncCol =
  | { type: 'u32'; data: number[] }
  | { type: 'u64'; data: Array<number | bigint> }
  | { type: 'f64'; data: number[] }
  | { type: 'str'; data: string[] };

const COL_TYPE_CODE = { f64: 3, str: 4, u32: 1, u64: 2 } as const;

function encodeFrame(
  kind: number,
  flags: number,
  requestId: number | bigint,
  epoch: number | bigint,
  cols: EncCol[],
): ArrayBuffer {
  const nrows = cols[0]?.data.length ?? 0;
  for (const col of cols) {
    if (col.data.length !== nrows) throw new Error('columns must share nrows');
  }
  const utf8 = new TextEncoder();

  // Column payloads with offsets relative to payload start.
  let payload: number[] = [];
  const pad8 = () => {
    while (payload.length % 8 !== 0) payload.push(0);
  };
  const pushU32 = (value: number) => {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, value, true);
    payload = payload.concat([...b]);
  };
  const pushU64 = (value: number | bigint) => {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(value), true);
    payload = payload.concat([...b]);
  };
  const pushF64 = (value: number) => {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setFloat64(0, value, true);
    payload = payload.concat([...b]);
  };

  const colOffsets: number[] = [];
  for (const col of cols) {
    pad8();
    colOffsets.push(payload.length);
    switch (col.type) {
      case 'u32':
        for (const x of col.data) pushU32(x);
        break;
      case 'u64':
        for (const x of col.data) pushU64(x);
        break;
      case 'f64':
        for (const x of col.data) pushF64(x);
        break;
      case 'str': {
        let at = 0;
        pushU32(0);
        const bytes: number[] = [];
        for (const s of col.data) {
          const encoded = utf8.encode(s);
          at += encoded.length;
          bytes.push(...encoded);
          pushU32(at);
        }
        pad8();
        payload = payload.concat(bytes);
        break;
      }
    }
  }
  pad8();

  const dirLen = cols.length * 16;
  const payloadLen = dirLen + payload.length;
  const total = 40 + payloadLen + 8;
  const out = new Uint8Array(total);
  const view = new DataView(out.buffer);

  // Header
  out.set([0x42, 0x51, 0x46, 0x31], 0); // "BQF1"
  view.setUint16(4, kind, true);
  view.setUint16(6, flags, true);
  view.setBigUint64(8, BigInt(requestId), true);
  view.setBigUint64(16, BigInt(epoch), true);
  view.setUint32(24, cols.length, true);
  view.setUint32(28, nrows, true);
  view.setBigUint64(32, BigInt(payloadLen), true);

  // Directory
  cols.forEach((col, i) => {
    view.setUint8(40 + i * 16, COL_TYPE_CODE[col.type]);
    view.setBigUint64(40 + i * 16 + 8, BigInt(colOffsets[i]!), true);
  });

  // Payload + trailer
  out.set(payload, 40 + dirLen);
  view.setBigUint64(
    total - 8,
    BigInt(crc32c(out.subarray(0, total - 8))),
    true,
  );
  return out.buffer;
}

// ---------------------------------------------------------------------------

describe('bqf1 crc32c', () => {
  it('matches the RFC 3720 test vectors (same as the Rust implementation)', () => {
    expect(crc32c(new TextEncoder().encode('123456789'))).toBe(0xe3069283);
    expect(crc32c(new Uint8Array(32))).toBe(0x8a9136aa);
    expect(crc32c(new Uint8Array(32).fill(0xff))).toBe(0x62a8ab43);
    expect(crc32c(new Uint8Array(0))).toBe(0);
  });
});

describe('bqf1 decodeFrame', () => {
  it('round-trips all column types', () => {
    const buf = encodeFrame(FrameKind.TopFunctions, 0b010, 42, 7, [
      { data: [1, 2, 0xffffffff], type: 'u32' },
      { data: [10n, 20n, 9007199254740991n], type: 'u64' },
      { data: [0.5, -1.5, 2.25], type: 'f64' },
      { data: ['alpha', '', 'γreeké'], type: 'str' },
    ]);

    const frame = decodeFrame(buf);
    expect(frame.kind).toBe(FrameKind.TopFunctions);
    expect(frame.flags).toBe(0b010);
    expect(frame.requestId).toBe(42);
    expect(frame.epoch).toBe(7);
    expect(frame.nrows).toBe(3);
    expect(frame.cols).toHaveLength(4);

    expect(frame.cols[0]).toMatchObject({ type: 'u32' });
    expect([...(frame.cols[0]!.data as Uint32Array)]).toEqual([
      1, 2, 0xffffffff,
    ]);
    expect(frame.cols[1]).toMatchObject({ type: 'u64' });
    expect([...(frame.cols[1]!.data as BigUint64Array)]).toEqual([
      10n,
      20n,
      9007199254740991n,
    ]);
    expect(frame.cols[2]).toMatchObject({ type: 'f64' });
    expect([...(frame.cols[2]!.data as Float64Array)]).toEqual([
      0.5, -1.5, 2.25,
    ]);
    expect(frame.cols[3]).toMatchObject({ type: 'str' });
    expect(frame.cols[3]!.data).toEqual(['alpha', '', 'γreeké']);
  });

  it('produces aligned zero-copy views over the frame buffer', () => {
    const buf = encodeFrame(FrameKind.TopFunctions, 0, 1, 0, [
      { data: ['x'], type: 'str' }, // odd utf8 length exercises padding
      { data: [123n], type: 'u64' },
      { data: [4.5], type: 'f64' },
    ]);
    const frame = decodeFrame(buf);
    const u64 = frame.cols[1]!.data as BigUint64Array;
    const f64 = frame.cols[2]!.data as Float64Array;
    expect(u64.buffer).toBe(buf); // zero-copy
    expect(f64.buffer).toBe(buf);
    expect(u64.byteOffset % 8).toBe(0);
    expect(f64.byteOffset % 8).toBe(0);
    expect(u64[0]).toBe(123n);
    expect(f64[0]).toBe(4.5);
  });

  it('rejects a corrupted frame (CRC mismatch)', () => {
    const buf = encodeFrame(FrameKind.Status, 0, 1, 0, [
      { data: [9], type: 'u32' },
      { data: ['boom'], type: 'str' },
    ]);
    const corrupted = new Uint8Array(buf.slice(0));
    corrupted[Math.floor(corrupted.length / 2)] ^= 0xff;
    expect(() => decodeFrame(corrupted.buffer)).toThrow(BqfDecodeError);
    expect(() => decodeFrame(corrupted.buffer)).toThrow(/CRC/);
  });

  it('rejects bad magic and truncated frames', () => {
    const buf = encodeFrame(FrameKind.Status, 0, 1, 0, [
      { data: [0], type: 'u32' },
      { data: ['ok'], type: 'str' },
    ]);
    const badMagic = new Uint8Array(buf.slice(0));
    badMagic[0] = 0x41;
    expect(() => decodeFrame(badMagic.buffer)).toThrow(/magic/);
    expect(() => decodeFrame(buf.slice(0, 20))).toThrow(/too short/);
  });

  it('decodes a RunsList fixture into named typed columns', () => {
    const buf = encodeFrame(FrameKind.RunsList, 0, 5, 3, [
      { data: ['proj/b-0001', 'proj/b-0002'], type: 'str' },
      { data: ['b-0001', 'b-0002'], type: 'str' },
      { data: ['ExtractResume', 'ClassifyTicket'], type: 'str' },
      { data: ['playground', 'cli'], type: 'str' },
      { data: ['succeeded', 'running'], type: 'str' },
      { data: ['rev-abcdef1234567890', ''], type: 'str' },
      { data: [1753900000000n, 1753900050000n], type: 'u64' },
      { data: [1753900001500n, 0n], type: 'u64' },
      { data: [1, 0], type: 'u32' },
    ]);

    const frame = decodeFrame(buf);
    expect(frame.requestId).toBe(5);
    const runs = asRunsList(frame);
    expect(runs.runKey).toEqual(['proj/b-0001', 'proj/b-0002']);
    expect(runs.boundaryId).toEqual(['b-0001', 'b-0002']);
    expect(runs.target).toEqual(['ExtractResume', 'ClassifyTicket']);
    expect(runs.source).toEqual(['playground', 'cli']);
    expect(runs.status).toEqual(['succeeded', 'running']);
    expect(runs.revision).toEqual(['rev-abcdef1234567890', '']);
    expect([...runs.createdMs]).toEqual([1753900000000, 1753900050000]);
    expect([...runs.completedMs]).toEqual([1753900001500, 0]);
    expect([...runs.hasSnapshot]).toEqual([1, 0]);
  });

  it('decodes an empty RunsList frame (no history dirs in dev)', () => {
    const buf = encodeFrame(FrameKind.RunsList, 0, 9, 0, [
      { data: [], type: 'str' },
      { data: [], type: 'str' },
      { data: [], type: 'str' },
      { data: [], type: 'str' },
      { data: [], type: 'str' },
      { data: [], type: 'str' },
      { data: [], type: 'u64' },
      { data: [], type: 'u64' },
      { data: [], type: 'u32' },
    ]);
    const runs = asRunsList(decodeFrame(buf));
    expect(runs.runKey).toEqual([]);
    expect(runs.createdMs.length).toBe(0);
  });

  it('typed helpers enforce frame kind and expose fold sentinels', () => {
    const leftHeavy = encodeFrame(FrameKind.LeftHeavy, 0, 2, 0, [
      { data: [0, 1, 1], type: 'u32' },
      { data: [7, 8, FOLD_ROW_FUNCTION], type: 'u32' },
      { data: [1000n, 600n, 400n], type: 'u64' },
      { data: [0n, 600n, 400n], type: 'u64' },
      { data: [1n, 3n, 5n], type: 'u64' },
      { data: [0n, 1n, 0n], type: 'u64' },
      { data: [0, 0, 4], type: 'u32' },
    ]);
    const frame = decodeFrame(leftHeavy);
    const rows = asLeftHeavy(frame);
    expect([...rows.depth]).toEqual([0, 1, 1]);
    expect(rows.functionId[2]).toBe(FOLD_ROW_FUNCTION);
    expect([...rows.totalNs]).toEqual([1000, 600, 400]);
    expect([...rows.foldedCount]).toEqual([0, 0, 4]);
    expect(() => asStatus(frame)).toThrow(/expected Status/);
  });

  it('decodes a Status frame', () => {
    const buf = encodeFrame(FrameKind.Status, 0, 11, 0, [
      { data: [404], type: 'u32' },
      { data: ['run not found'], type: 'str' },
    ]);
    const status = asStatus(decodeFrame(buf));
    expect(status.code[0]).toBe(404);
    expect(status.message[0]).toBe('run not found');
  });
});
