import assert from 'node:assert/strict';
import test from 'node:test';
import { readRunResult } from '../lib/baml-runner/result.mjs';
import { decodeBase64, decodeOutboundValue, formatValue } from '../lib/baml-runner/outbound.mjs';

const world = 'GgV3b3JsZA==';

test('decodes the inline result shape emitted by BAML 0.18', async () => {
  const value = await readRunResult({
    boundaryId: 'run-1',
    outcome: {
      status: 'succeeded',
      result: {
        rendererHint: 'baml.outbound.base64',
        value: world,
        valueRef: null,
      },
    },
  });

  assert.equal(value, 'world');
  assert.equal(formatValue(value), '"world"');
});

test('decodes the value-reference result shape emitted by BAML 0.17', async () => {
  const calls = [];
  const value = await readRunResult({
    boundaryId: 'run-2',
    outcome: {
      status: 'succeeded',
      result: {
        rendererHint: 'baml.outbound.base64',
        valueRef: { id: 'value-1' },
      },
    },
    readValue: async (boundaryId, valueRef) => {
      calls.push({ boundaryId, valueRef });
      return { bodyBase64: world };
    },
  });

  assert.equal(value, 'world');
  assert.deepEqual(calls, [
    { boundaryId: 'run-2', valueRef: { id: 'value-1' } },
  ]);
});

test('fails loudly for an unknown renderer instead of showing the wrong value', async () => {
  await assert.rejects(
    readRunResult({
      boundaryId: 'run-3',
      outcome: {
        status: 'succeeded',
        result: { rendererHint: 'unknown', value: world },
      },
    }),
    /unsupported BAML result renderer/,
  );
});

test('decodes representative scalar protobuf values', () => {
  assert.equal(decodeOutboundValue(decodeBase64('EgA=')), null);
  assert.equal(decodeOutboundValue(decodeBase64('ICo=')), 42n);
  assert.equal(decodeOutboundValue(decodeBase64('MAE=')), true);
});
