import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { cp, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { buildReviewBundle, parseSummary, writeReviewBundle } from '../scripts/review-baml-book.mjs';

const execFileAsync = promisify(execFile);
const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureRoot = path.join(packageRoot, 'tests', 'fixtures', 'book');

test('parses ordered mdBook chapters and assigns stable routes', () => {
  assert.deepEqual(parseSummary(`# Book\n\n[Title](title-page.md)\n- [Getting Started](ch01-00-getting-started.md)\n`), [
    { title: 'Title', source: 'src/title-page.md', output: 'title-page.mdx' },
    { title: 'Getting Started', source: 'src/ch01-00-getting-started.md', output: 'getting-started.mdx' },
  ]);
  assert.throws(() => parseSummary('[Escape](../private.md)'), /escapes src/);
});

test('produces an unapproved, reproducible review bundle from a clean pinned checkout', async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'baml-book-review-'));
  const sourceRoot = path.join(temporaryRoot, 'book');
  const outputRoot = path.join(temporaryRoot, 'review');
  await cp(fixtureRoot, sourceRoot, { recursive: true });
  await writeFile(path.join(sourceRoot, 'src', 'SUMMARY.md'), '# Book\n\n- [Getting Started](chapter.md)\n');
  await execFileAsync('git', ['init', '-q'], { cwd: sourceRoot });
  await execFileAsync('git', ['add', '.'], { cwd: sourceRoot });
  await execFileAsync('git', ['-c', 'user.name=Docs Test', '-c', 'user.email=docs@example.com', 'commit', '-qm', 'fixture'], { cwd: sourceRoot });
  const { stdout } = await execFileAsync('git', ['rev-parse', 'HEAD'], { cwd: sourceRoot });
  const manifestPath = path.join(temporaryRoot, 'book-import.json');
  await writeFile(manifestPath, JSON.stringify({
    schemaVersion: 1,
    source: { repository: 'https://example.com/baml-book', revision: stdout.trim() },
    chapters: [],
  }));

  const bundle = await buildReviewBundle({ manifestPath, sourceRoot });
  assert.equal(bundle.disposition, 'candidate-only');
  assert.equal(bundle.candidates.length, 1);
  assert.equal(bundle.candidates[0].approvalEntry.status, 'approved');
  assert.equal(bundle.candidates[0].stats.runnableListings, 1);
  assert.match(bundle.candidates[0].convertedContent, /<BamlRunner files=/);
  await writeReviewBundle({ bundle, outputRoot });

  const review = await readFile(path.join(outputRoot, 'README.md'), 'utf8');
  const serialized = JSON.parse(await readFile(path.join(outputRoot, 'review.json'), 'utf8'));
  assert.match(review, /None of these chapters is approved or published/);
  assert.match(review, /Approval entry to use only after editorial sign-off/);
  assert.equal(serialized.candidates[0].convertedContent, undefined);
  assert.equal(serialized.candidates[0].sourceSha256.length, 64);
  assert.equal(serialized.candidates[0].convertedSha256.length, 64);
});

test('refuses a dirty source checkout before preparing candidates', async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'baml-book-review-dirty-'));
  const sourceRoot = path.join(temporaryRoot, 'book');
  await cp(fixtureRoot, sourceRoot, { recursive: true });
  await writeFile(path.join(sourceRoot, 'src', 'SUMMARY.md'), '- [Getting Started](chapter.md)\n');
  await execFileAsync('git', ['init', '-q'], { cwd: sourceRoot });
  await execFileAsync('git', ['add', '.'], { cwd: sourceRoot });
  await execFileAsync('git', ['-c', 'user.name=Docs Test', '-c', 'user.email=docs@example.com', 'commit', '-qm', 'fixture'], { cwd: sourceRoot });
  const { stdout } = await execFileAsync('git', ['rev-parse', 'HEAD'], { cwd: sourceRoot });
  const manifestPath = path.join(temporaryRoot, 'book-import.json');
  await writeFile(manifestPath, JSON.stringify({ schemaVersion: 1, source: { repository: 'x', revision: stdout.trim() }, chapters: [] }));
  await writeFile(path.join(sourceRoot, 'src', 'chapter.md'), '# Changed after review\n');

  await assert.rejects(buildReviewBundle({ manifestPath, sourceRoot }), /uncommitted files/);
});
