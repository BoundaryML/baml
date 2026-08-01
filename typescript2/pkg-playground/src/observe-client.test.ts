import { describe, expect, it } from 'vitest';

import { type BqfColumn, type BqfFrame, BqfFrameKind } from './observe-bqf';
import {
  decodeDiffFrame,
  decodeSearchFrame,
  decodeValueDagFrame,
} from './observe-client';

function frame(
  kind: BqfFrameKind,
  columns: ReadonlyMap<number, BqfColumn>,
): BqfFrame {
  return {
    bytes: new ArrayBuffer(0),
    columns,
    dataEpoch: 1n,
    flags: 0,
    kind,
    requestId: 1n,
    rowCount: 1,
  };
}

describe('observability advanced frame decoders', () => {
  it('decodes revision-backed search identities and aggregate counters', () => {
    const decoded = decodeSearchFrame(
      frame(
        BqfFrameKind.Search,
        new Map<number, BqfColumn>([
          [1, new Uint32Array([16])],
          [2, ['function:user.extract']],
          [3, ['user.extract']],
          [4, new Uint32Array([3])],
          [5, new Uint8Array([2])],
          [10, new BigUint64Array([8n])],
          [11, new BigUint64Array([1n])],
          [12, new BigUint64Array([90n])],
          [13, new BigUint64Array([60n])],
          [14, new BigUint64Array([30n])],
        ]),
      ),
    );

    expect(decoded[0]).toMatchObject({
      calls: 8n,
      definitionKey: 'function:user.extract',
      errors: 1n,
      fqn: 'user.extract',
      functionId: 16,
    });
  });

  it('decodes signed aligned diffs and missing dense ids', () => {
    const decoded = decodeDiffFrame(
      frame(
        BqfFrameKind.Diff,
        new Map<number, BqfColumn>([
          [1, ['function:user.extract']],
          [2, ['user.extract']],
          [3, new Uint32Array([0xffffffff])],
          [4, new Uint32Array([16])],
          [5, new Uint8Array([1])],
          [6, new Uint8Array([1])],
          [10, new BigInt64Array([4n])],
          [11, new BigInt64Array([1n])],
          [12, new BigInt64Array([-90n])],
          [13, new BigInt64Array([-60n])],
          [14, new BigInt64Array([-30n])],
        ]),
      ),
    );

    expect(decoded[0]).toMatchObject({
      definitionChanged: true,
      deltaCalls: 4n,
      deltaTotalNs: -90n,
      leftFunctionId: null,
      presence: 1,
      rightFunctionId: 16,
    });
  });

  it('decodes bounded value DAG navigation and diff rows', () => {
    const decoded = decodeValueDagFrame(
      frame(
        BqfFrameKind.ValueDag,
        new Map<number, BqfColumn>([
          [1, new Uint8Array([4])],
          [2, ['a'.repeat(64)]],
          [3, ['b'.repeat(64)]],
          [4, new Uint16Array([3])],
          [5, new Uint32Array([7])],
          [6, new BigUint64Array([0xffffffffffffffffn])],
          [7, new Uint8Array([0])],
          [8, new Uint8Array([1])],
        ]),
      ),
    );

    expect(decoded[0]).toEqual({
      canonicalLoaded: true,
      depth: 3,
      equal: false,
      kind: 4,
      logicalLength: null,
      ordinal: 7,
      primaryCid: 'a'.repeat(64),
      secondaryCid: 'b'.repeat(64),
    });
  });
});
