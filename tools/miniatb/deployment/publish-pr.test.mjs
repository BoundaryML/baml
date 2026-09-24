import { test } from 'node:test';
import assert from 'node:assert/strict';
import { publishPR, prepareCommit } from './publish-pr.mjs';
const state = { repository: 'example/baml-fork', branch: 'bammy/test-issue', sha: 'a'.repeat(40), title: 'Expected: fix', body: 'verified', phase: 'prepared' };
const pr = { state: 'open', head: { sha: state.sha, repo: { full_name: state.repository } }, html_url: 'https://github.com/BoundaryML/baml/pull/123' };
function remote({ branch = false, existing = false, losePush = false, loseCreate = false, failSave = false, changed = false } = {}) {
  const counts = { push: 0, create: 0 }; const saved = [];
  return { counts, saved,
    api: async (method, route, body) => {
      if (method === 'GET' && route === '/repos/' + state.repository) return { full_name: state.repository, fork: true, private: false, source: { full_name: 'BoundaryML/baml' } };
      if (method === 'GET' && route.includes('/pulls?')) return existing ? [pr] : [];
      if (method === 'GET') { assert.equal(route, `/repos/${state.repository}/git/ref/heads/${state.branch}`); return branch ? { object: { sha: changed ? 'b'.repeat(40) : state.sha } } : null; }
      assert.equal(body.head, 'example:' + state.branch); assert.equal(body.head_repo, 'baml-fork'); assert.equal(body.maintainer_can_modify, false); assert.equal(route, '/repos/BoundaryML/baml/pulls'); assert.equal(body.draft, true); assert.equal(body.base, 'canary');
      counts.create++; existing = true;
      if (loseCreate) throw new Error('response lost');
      return pr;
    },
    push: async () => { counts.push++; branch = true; if (losePush) throw new Error('response lost'); },
    save: async s => { saved.push(s); if (failSave && s.phase === 'published') { failSave = false; throw new Error('disk interrupted'); } },
  };
}
test('fresh publication creates exactly one draft', async () => {
  const r = remote(); assert.equal(await publishPR(state, r), pr.html_url);
  assert.deepEqual(r.counts, { push: 1, create: 1 }); assert.equal(r.saved.at(-1).phase, 'published');
});
test('lost push and create responses reconcile remote state', async () => {
  const r = remote({ losePush: true, loseCreate: true });
  assert.equal(await publishPR(state, r), pr.html_url); assert.deepEqual(r.counts, { push: 1, create: 1 });
});
test('crash after PR creation resumes without another push or PR', async () => {
  const r = remote({ failSave: true }); await assert.rejects(publishPR(state, r));
  assert.equal(await publishPR(state, r), pr.html_url); assert.deepEqual(r.counts, { push: 1, create: 1 });
});
test('existing branch resumes but a changed branch is never overwritten', async () => {
  const r = remote({ branch: true }); await publishPR(state, r); assert.equal(r.counts.push, 0);
  const changed = remote({ branch: true, changed: true }); await assert.rejects(publishPR(state, changed), /refusing to overwrite/);
  assert.deepEqual(changed.counts, { push: 0, create: 0 });
});
test('missing recorded PR fails closed and invalid branch never reaches API', async () => {
  await assert.rejects(publishPR({ ...state, phase: 'published' }, remote()), /refusing duplicate/);
  await assert.rejects(publishPR({ ...state, branch: 'canary' }, remote()), /Invalid publication/);
});

test('prepared commit resumes before or after the local branch update and rejects intervening edits', () => {
  const prepared = { ...state, base: 'b'.repeat(40) };
  let head = prepared.base;
  let dirty = false;
  const git = args => {
    if (args[0] === 'branch') return state.branch;
    if (args[0] === 'rev-parse') return args[1] === 'HEAD' ? head : 'tree';
    if (args[0] === 'write-tree') return dirty ? 'changed-tree' : 'tree';
    if (args[0] === 'diff' || args[0] === 'status') return '';
    assert.deepEqual(args, ['update-ref', `refs/heads/${state.branch}`, state.sha, prepared.base]);
    head = state.sha;
    return '';
  };
  prepareCommit(prepared, git);
  assert.equal(head, state.sha);
  prepareCommit(prepared, git);
  head = prepared.base; dirty = true;
  assert.throws(() => prepareCommit(prepared, git), /source changed/);
});

test('publication rejects upstream, private repositories and unrelated forks before a push', async () => {
  for (const repository of [undefined, '', 'BoundaryML/baml', 'boundaryml/baml']) {
    const r = remote(); await assert.rejects(publishPR({ ...state, repository }, r), /dedicated fork/);
    assert.deepEqual(r.counts, { push: 0, create: 0 });
  }
  for (const fork of [
    { full_name: state.repository, fork: false, private: false, source: { full_name: 'BoundaryML/baml' } },
    { full_name: state.repository, fork: true, private: true, source: { full_name: 'BoundaryML/baml' } },
    { full_name: state.repository, fork: true, private: false, source: { full_name: 'other/repo' } },
  ]) {
    const r = remote(); r.api = async () => fork;
    await assert.rejects(publishPR(state, r), /public BAML fork/);
    assert.deepEqual(r.counts, { push: 0, create: 0 });
  }
});
