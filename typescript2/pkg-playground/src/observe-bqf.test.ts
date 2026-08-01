import { describe, expect, it } from 'vitest';

import { decodeBqlSchemaFrame } from './observe-client';
import { BqfFrameKind, decodeBqf1 } from './observe-bqf';

describe('BQF1 decoder', () => {
  it('decodes a bounded empty frame and rejects CRC corruption', () => {
    const bytes = new Uint8Array(44);
    bytes.set([0x42, 0x51, 0x46, 0x31]);
    const view = new DataView(bytes.buffer);
    view.setUint16(4, 1, true);
    view.setUint16(6, BqfFrameKind.Completeness, true);
    view.setUint32(8, 1 << 5, true);
    view.setUint16(12, 0, true);
    view.setUint32(16, 0, true);
    view.setBigUint64(20, 17n, true);
    view.setBigUint64(28, 23n, true);
    view.setUint32(36, 0, true);
    view.setUint32(40, crc32c(bytes.subarray(0, 40)), true);

    const frame = decodeBqf1(bytes.buffer);
    expect(frame.kind).toBe(BqfFrameKind.Completeness);
    expect(frame.requestId).toBe(17n);
    expect(frame.dataEpoch).toBe(23n);
    expect(frame.rowCount).toBe(0);

    bytes[9] ^= 1;
    expect(() => decodeBqf1(bytes.buffer)).toThrow('CRC mismatch');
  });

  it('decodes the BQL schema carried by a Query frame', () => {
    const schema = {
      version: 1,
      default_limit: 1000,
      hard_max_rows: 100000,
      hard_max_bytes: 1048576,
      set_kinds: ['run_set', 'table'],
      stages: [
        {
          name: 'runs',
          category: 'source',
          inputs: [],
          output: 'run_set',
          preserves_input: false,
          arguments: [],
          availability: 'implemented',
          description: 'bounded runs',
        },
      ],
      fields: [],
    };
    const bytes = utf8QueryFrame([
      'schema',
      JSON.stringify('schema'),
      '[]',
      JSON.stringify([schema]),
      JSON.stringify({
        complete: true,
        watermarks: [],
        capture_loss: [],
        sources_consulted: [],
        truncated: false,
        next_cursor: null,
        warnings: [],
        snapshot: 'bqsnap_1_',
      }),
    ]);
    expect(decodeBqlSchemaFrame(decodeBqf1(bytes))).toEqual(schema);
  });
});

function utf8QueryFrame(values: readonly string[]): ArrayBuffer {
  const encoder = new TextEncoder();
  const directoryBytes = values.length * 24;
  const payloadStart = 40 + directoryBytes;
  const columns = values.map((value) => {
    const utf8 = encoder.encode(value);
    return { utf8, dataOffset: 0, auxOffset: 0 };
  });
  let cursor = payloadStart;
  for (const column of columns) {
    cursor = align8(cursor);
    column.dataOffset = cursor;
    cursor += 8;
    cursor = align8(cursor);
    column.auxOffset = cursor;
    cursor += column.utf8.byteLength;
  }
  const bytes = new Uint8Array(cursor + 4);
  bytes.set([0x42, 0x51, 0x46, 0x31]);
  const view = new DataView(bytes.buffer);
  view.setUint16(4, 1, true);
  view.setUint16(6, BqfFrameKind.Query, true);
  view.setUint32(8, 1 << 5, true);
  view.setUint16(12, values.length, true);
  view.setUint32(16, 1, true);
  view.setBigUint64(20, 9n, true);
  view.setBigUint64(28, 0n, true);
  view.setUint32(36, directoryBytes, true);
  columns.forEach((column, index) => {
    const directory = 40 + index * 24;
    view.setUint16(directory, index + 1, true);
    view.setUint8(directory + 2, 6);
    view.setUint32(directory + 4, column.dataOffset, true);
    view.setUint32(directory + 8, 8, true);
    view.setUint32(directory + 12, column.auxOffset, true);
    view.setUint32(directory + 16, column.utf8.byteLength, true);
    view.setUint32(column.dataOffset, 0, true);
    view.setUint32(column.dataOffset + 4, column.utf8.byteLength, true);
    bytes.set(column.utf8, column.auxOffset);
  });
  view.setUint32(cursor, crc32c(bytes.subarray(0, cursor)), true);
  return bytes.buffer;
}

function align8(value: number): number {
  return (value + 7) & ~7;
}

const TABLE = (() => {
  const table = new Uint32Array(256);
  for (let index = 0; index < 256; index += 1) {
    let crc = index;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0x82f63b78 : 0);
    }
    table[index] = crc >>> 0;
  }
  return table;
})();

function crc32c(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc = (crc >>> 8) ^ (TABLE[(crc ^ byte) & 0xff] ?? 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
