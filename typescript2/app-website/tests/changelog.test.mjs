import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import {
  changeCount,
  parseChangelog,
  releaseId,
} from '../app/changelog/changelog.ts';

test('the copied language changelog parses into releases', async () => {
  const source = await readFile(
    path.join(process.cwd(), 'data/changelog.md'),
    'utf8',
  );
  const releases = parseChangelog(source);

  assert.ok(releases.length > 0);
  assert.match(releases[0].version, /^\d+\.\d+\.\d+/);
  assert.match(releases[0].date, /^\d{4}-\d{2}-\d{2}$/);
  assert.ok(changeCount(releases[0].body) > 0);
  assert.equal(
    new Set(releases.map((release) => releaseId(release.version))).size,
    releases.length,
  );
});
