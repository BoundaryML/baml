import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import { boundedTree } from './sandbox-run.mjs';
test('persistent storage bounds count bytes and entries without following links', () => {
  const dir = fs.mkdtempSync(os.tmpdir() + '/miniatb-bounds-');
  try {
    fs.writeFileSync(dir + '/file', 'abcd');
    fs.symlinkSync('/not-an-agent-readable-directory', dir + '/link');
    boundedTree(dir, 4, 2);
    assert.throws(() => boundedTree(dir, 3), /storage limit/);
    assert.throws(() => boundedTree(dir, 10, 1), /entry limit/);
  } finally { fs.rmSync(dir, { recursive: true }); }
});
