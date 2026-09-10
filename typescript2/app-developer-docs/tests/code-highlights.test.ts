import assert from 'node:assert/strict';
import { test } from 'node:test';
import { resolveCodeHighlights } from '../lib/snippets/code-highlights';

test('highlights reject stale, ambiguous, and overlapping selections', () => {
  assert.throws(() =>
    resolveCodeHighlights('call()', [{ kind: 'success', text: 'missing' }]),
  );
  assert.throws(() =>
    resolveCodeHighlights('x + x', [{ kind: 'success', text: 'x' }]),
  );
  assert.deepEqual(
    resolveCodeHighlights('x + x', [
      { kind: 'success', occurrence: 2, text: 'x' },
    ]),
    [{ end: 5, kind: 'success', start: 4 }],
  );
  assert.deepEqual(
    resolveCodeHighlights('x + x', [
      { kind: 'success', occurrence: 1, text: 'x' },
    ]),
    [{ end: 1, kind: 'success', start: 0 }],
  );
  assert.throws(() =>
    resolveCodeHighlights('call()', [
      { kind: 'success', text: 'call()' },
      { kind: 'syntax', text: '()' },
    ]),
  );
});
