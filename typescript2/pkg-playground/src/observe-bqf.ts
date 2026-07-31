export const BQF1_VERSION = 1;
export const BQF1_HEADER_BYTES = 40;
export const BQF1_DIRECTORY_BYTES = 24;
export const BQF1_CRC_BYTES = 4;

export const enum BqfFrameKind {
  Timeline = 1,
  LeftHeavy = 2,
  Runs = 3,
  RunMeta = 4,
  Completeness = 5,
  Sandwich = 6,
  Search = 7,
  Diff = 8,
  ValueRefs = 9,
  ValueDag = 10,
  Query = 11,
}

export const enum BqfColumnType {
  U8 = 1,
  U16 = 2,
  U32 = 3,
  U64 = 4,
  I64 = 5,
  Utf8 = 6,
}

export type BqfColumn =
  | Uint8Array
  | Uint16Array
  | Uint32Array
  | BigUint64Array
  | BigInt64Array
  | readonly string[];

export type BqfFrame = {
  readonly bytes: ArrayBuffer;
  readonly kind: BqfFrameKind;
  readonly flags: number;
  readonly requestId: bigint;
  readonly dataEpoch: bigint;
  readonly rowCount: number;
  readonly columns: ReadonlyMap<number, BqfColumn>;
};

export function decodeBqf1(input: ArrayBuffer): BqfFrame {
  if (input.byteLength < BQF1_HEADER_BYTES + BQF1_CRC_BYTES) {
    throw new Error('truncated BQF1 frame');
  }
  const bytes = new Uint8Array(input);
  if (
    bytes[0] !== 0x42 ||
    bytes[1] !== 0x51 ||
    bytes[2] !== 0x46 ||
    bytes[3] !== 0x31
  ) {
    throw new Error('invalid BQF1 magic');
  }
  const view = new DataView(input);
  if (view.getUint16(4, true) !== BQF1_VERSION) {
    throw new Error('unsupported BQF1 version');
  }
  const columnCount = view.getUint16(12, true);
  const rowCount = view.getUint32(16, true);
  const directoryLength = view.getUint32(36, true);
  if (directoryLength !== columnCount * BQF1_DIRECTORY_BYTES) {
    throw new Error('invalid BQF1 column directory length');
  }
  const crcOffset = input.byteLength - BQF1_CRC_BYTES;
  if (view.getUint32(crcOffset, true) !== crc32c(bytes.subarray(0, crcOffset))) {
    throw new Error('BQF1 CRC mismatch');
  }

  const columns = new Map<number, BqfColumn>();
  for (let index = 0; index < columnCount; index += 1) {
    const directory = BQF1_HEADER_BYTES + index * BQF1_DIRECTORY_BYTES;
    const id = view.getUint16(directory, true);
    const type = view.getUint8(directory + 2) as BqfColumnType;
    const dataOffset = view.getUint32(directory + 4, true);
    const dataLength = view.getUint32(directory + 8, true);
    const auxOffset = view.getUint32(directory + 12, true);
    const auxLength = view.getUint32(directory + 16, true);
    validateRegion(input.byteLength, dataOffset, dataLength);
    if (auxLength !== 0 || type === BqfColumnType.Utf8) {
      validateRegion(input.byteLength, auxOffset, auxLength);
    }
    switch (type) {
      case BqfColumnType.U8:
        requireLength(dataLength, rowCount);
        columns.set(id, new Uint8Array(input, dataOffset, rowCount));
        break;
      case BqfColumnType.U16:
        requireLength(dataLength, rowCount * 2);
        columns.set(id, new Uint16Array(input, dataOffset, rowCount));
        break;
      case BqfColumnType.U32:
        requireLength(dataLength, rowCount * 4);
        columns.set(id, new Uint32Array(input, dataOffset, rowCount));
        break;
      case BqfColumnType.U64:
        requireLength(dataLength, rowCount * 8);
        columns.set(id, new BigUint64Array(input, dataOffset, rowCount));
        break;
      case BqfColumnType.I64:
        requireLength(dataLength, rowCount * 8);
        columns.set(id, new BigInt64Array(input, dataOffset, rowCount));
        break;
      case BqfColumnType.Utf8: {
        requireLength(dataLength, (rowCount + 1) * 4);
        const offsets = new Uint32Array(input, dataOffset, rowCount + 1);
        const utf8 = new Uint8Array(input, auxOffset, auxLength);
        const decoder = new TextDecoder();
        const strings = Array.from({ length: rowCount }, (_, row) => {
          const start = offsets[row] ?? 0;
          const end = offsets[row + 1] ?? start;
          if (end < start || end > utf8.byteLength) {
            throw new Error('invalid BQF1 UTF-8 offsets');
          }
          return decoder.decode(utf8.subarray(start, end));
        });
        columns.set(id, strings);
        break;
      }
      default:
        throw new Error(`unknown BQF1 column type ${type}`);
    }
  }

  return {
    bytes: input,
    kind: view.getUint16(6, true) as BqfFrameKind,
    flags: view.getUint32(8, true),
    requestId: view.getBigUint64(20, true),
    dataEpoch: view.getBigUint64(28, true),
    rowCount,
    columns,
  };
}

export function bqfColumn<T extends BqfColumn>(
  frame: BqfFrame,
  id: number,
): T {
  const column = frame.columns.get(id);
  if (column == null) throw new Error(`BQF1 frame is missing column ${id}`);
  return column as T;
}

function validateRegion(total: number, offset: number, length: number): void {
  if (
    offset < BQF1_HEADER_BYTES ||
    offset % 8 !== 0 ||
    offset + length > total - BQF1_CRC_BYTES
  ) {
    throw new Error('BQF1 column is out of bounds or misaligned');
  }
}

function requireLength(actual: number, expected: number): void {
  if (actual !== expected) {
    throw new Error(`invalid BQF1 column length ${actual}, expected ${expected}`);
  }
}

const CRC32C_TABLE = (() => {
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
    crc = (crc >>> 8) ^ (CRC32C_TABLE[(crc ^ byte) & 0xff] ?? 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
