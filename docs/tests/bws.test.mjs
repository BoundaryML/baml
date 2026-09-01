import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const bwsRoot = path.join(packageRoot, 'content', 'bws');

test('BWS expands to Boundary Web Services on its pre-release surface', async () => {
  const [landing, meta] = await Promise.all([
    readFile(path.join(bwsRoot, 'index.mdx'), 'utf8'),
    readFile(path.join(bwsRoot, 'meta.json'), 'utf8'),
  ]);

  assert.match(landing, /title: Boundary Web Services/);
  assert.match(landing, /Boundary Web Services \(BWS\)/);
  assert.match(landing, /BWS\) is not yet available/);
  assert.match(meta, /"title": "Boundary Web Services"/);
  assert.doesNotMatch(landing, /Boundary Workflow Service/);
});

test('the BWS navigation exposes only the truthful landing page', async () => {
  const meta = JSON.parse(await readFile(path.join(bwsRoot, 'meta.json'), 'utf8'));
  assert.deepEqual(meta.pages, ['index']);
  await access(path.join(bwsRoot, 'index.mdx'));
});

test('the pre-release page does not imply a usable BWS workflow', async () => {
  const content = await readFile(path.join(bwsRoot, 'index.mdx'), 'utf8');
  assert.match(content, /There is no onboarding, API reference, deployment guide/);
  assert.doesNotMatch(content, /baml-cli deploy|BOUNDARY_API_KEY|api2\.boundaryml\.com/);
});
