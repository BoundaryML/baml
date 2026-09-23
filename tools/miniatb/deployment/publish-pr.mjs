import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export async function publishPR(state, deps) {
  if (!/^bammy\/[a-z0-9-]+$/.test(state.branch) || !/^[a-f0-9]{40}$/.test(state.sha)) throw new Error('Invalid publication identity');
  const head = `BoundaryML:${state.branch}`;
  async function existingPR() {
    const rows = await deps.api('GET', `/repos/BoundaryML/baml/pulls?state=all&head=${encodeURIComponent(head)}&base=canary&per_page=100`);
    if (!Array.isArray(rows) || rows.length > 1) throw new Error('Ambiguous PR history');
    const pr = rows[0];
    if (!pr) return null;
    if (pr.state !== 'open' || pr.head?.sha !== state.sha || pr.head?.repo?.full_name !== 'BoundaryML/baml') throw new Error('Existing PR changed or closed; inspect before retrying');
    if (!/^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/.test(pr.html_url)) throw new Error('Invalid PR URL');
    return pr.html_url;
  }
  async function saved(pr) {
    await deps.save({ ...state, phase: 'published', pr });
    return pr;
  }
  const prior = await existingPR();
  if (prior) return saved(prior);
  if (state.phase === 'published') throw new Error('Recorded PR unavailable; refusing duplicate creation');
  const ref = `/repos/BoundaryML/baml/git/ref/heads/${state.branch}`;
  let remote = await deps.api('GET', ref, undefined, true);
  if (remote && remote.object?.sha !== state.sha) throw new Error('Remote branch changed; refusing to overwrite');
  if (!remote) {
    try { await deps.push(state.branch, state.sha); }
    catch (error) {
      remote = await deps.api('GET', ref, undefined, true);
      if (remote?.object?.sha !== state.sha) throw error;
    }
  }
  await deps.save({ ...state, phase: 'pushed', pr: null });
  // Reconcile again in case an earlier create succeeded before its response was lost.
  const found = await existingPR();
  if (found) return saved(found);
  try {
    await deps.api('POST', '/repos/BoundaryML/baml/pulls', {
      head: state.branch, base: 'canary', title: state.title, body: state.body, draft: true,
    });
  } catch (error) {
    const recovered = await existingPR();
    if (recovered) return saved(recovered);
    throw error;
  }
  const created = await existingPR();
  if (!created) throw new Error('PR creation not confirmed; retry reconciliation');
  return saved(created);
}

export function prepareCommit(state, git) {
  const { branch, sha, base } = state;
  if (!/^bammy\/[a-z0-9-]+$/.test(branch) || !/^[a-f0-9]{40}$/.test(sha) || git(['branch', '--show-current']) !== branch) throw new Error('Prepared branch changed');
  const head = git(['rev-parse', 'HEAD']);
  if (head !== sha) {
    if (!/^[a-f0-9]{40}$/.test(base) || head !== base || git(['write-tree']) !== git(['rev-parse', `${sha}^{tree}`]) || git(['diff', '--name-only'])) throw new Error('Prepared source changed');
    git(['update-ref', `refs/heads/${branch}`, sha, base]);
  }
  if (git(['status', '--porcelain'])) throw new Error('Prepared commit changed');
}

async function main(dir, branch) {
  const file = path.join(dir, 'publication.json');
  const state = JSON.parse(fs.readFileSync(file, 'utf8'));
  if (state.branch !== branch || fs.lstatSync(file).isSymbolicLink()) throw new Error('Publication record mismatch');
  const tree = path.join(dir, 'worktree');
  const git = args => execFileSync('git', ['-c', 'core.hooksPath=/dev/null', '-C', tree, ...args], { encoding: 'utf8', timeout: 300000, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  prepareCommit(state, git);

  const token = process.env.GH_TOKEN;
  if (!token) throw new Error('Publishing credential missing');
  return publishPR(state, {
    api: async (method, route, body, missing = false) => {
      const r = await fetch('https://api.github.com' + route, {
        method, redirect: 'error', signal: AbortSignal.timeout(30000),
        headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'miniatb', 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (missing && r.status === 404) return null;
      if (!r.ok) throw new Error(`GitHub publication HTTP ${r.status}`);
      return r.json();
    },
    push: async (name, sha) => { git(['push', `--force-with-lease=refs/heads/${name}:`, 'https://github.com/BoundaryML/baml.git', `${sha}:refs/heads/${name}`]); },
    save: async value => { fs.writeFileSync(file + '.tmp', JSON.stringify(value), { mode: 0o600 }); fs.renameSync(file + '.tmp', file); },
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv[2], process.argv[3]).then(url => process.stdout.write(url)).catch(() => {
    process.stderr.write('Publication not confirmed; retained publication.json for reconciliation.\n');
    process.exitCode = 1;
  });
}
