import assert from 'node:assert/strict';
import test from 'node:test';

import {
  parseOperatorArguments,
  requireOperatorValue,
} from '../scripts/operator-arguments.ts';

test('operator arguments reject unknown, duplicate, and positional values', () => {
  assert.throws(() => parseOperatorArguments(['target'], [], []), /positional/);
  assert.throws(
    () => parseOperatorArguments(['--unknown'], [], []),
    /Unknown option/,
  );
  assert.throws(
    () =>
      parseOperatorArguments(
        ['--version', 'one', '--version', 'two'],
        ['version'],
        [],
      ),
    /Duplicate option/,
  );
});

test('operator arguments keep values and explicit flags separate', () => {
  const parsed = parseOperatorArguments(
    ['--version', '0.18.1', '--apply'],
    ['version'],
    ['apply'],
  );
  assert.equal(requireOperatorValue(parsed, 'version'), '0.18.1');
  assert.equal(parsed.flags.has('apply'), true);
});
