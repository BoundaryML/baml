import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { load } from 'js-yaml';
import { z } from 'zod';

const workflow = z
  .object({
    jobs: z.record(
      z.string(),
      z.object({
        if: z.string().optional(),
        name: z.string(),
        needs: z.union([z.string(), z.array(z.string())]).optional(),
        steps: z.array(
          z.object({
            env: z
              .record(
                z.string(),
                z.union([z.string(), z.number()]).transform(String),
              )
              .optional(),
            id: z.string().optional(),
            run: z.string().optional(),
          }),
        ),
      }),
    ),
    on: z.object({
      merge_group: z.object({ types: z.array(z.string()) }),
      pull_request: z.null(),
    }),
  })
  .parse(
    load(
      readFileSync(
        new URL(
          '../../../.github/workflows/developer-docs.yml',
          import.meta.url,
        ),
        'utf8',
      ),
    ),
  );

test('required Developer Docs check gates PRs and merge groups', () => {
  assert.ok('pull_request' in workflow.on && 'merge_group' in workflow.on);
  const gate = workflow.jobs['pre-merge-gate'];
  assert.equal(gate.name, 'Developer Docs');
  assert.equal(gate.if, 'always()');
  assert.ok(gate.needs?.includes('snippets'));
  assert.ok(gate.needs?.includes('production-path'));
  const run = gate.steps[0].run;
  assert.ok(run);
  const success = {
    AUTHORED_CONTENT_RESULT: 'success',
    CAN_USE_SECRETS: 'true',
    DATABASE_INTEGRATION_RESULT: 'success',
    DETERMINE_CHANGES_RESULT: 'success',
    PRODUCTION_PATH_RESULT: 'success',
    QUALITY_RESULT: 'success',
    SHOULD_RUN: 'true',
    SNIPPETS_RESULT: 'success',
  };
  for (const event of ['pull_request', 'merge_group']) {
    const check = (overrides: Record<string, string>): number | null =>
      spawnSync('bash', ['-c', run], {
        encoding: 'utf8',
        env: {
          ...process.env,
          ...success,
          GITHUB_EVENT_NAME: event,
          ...overrides,
        },
      }).status;
    assert.equal(check({}), 0);
    for (const key of Object.keys(success).filter((key) =>
      key.endsWith('_RESULT'),
    )) {
      assert.equal(
        check({ [key]: 'failure' }),
        1,
        `${event}: ${key} must block merging`,
      );
      assert.equal(
        check({ [key]: 'cancelled' }),
        1,
        `${event}: ${key} must block merging`,
      );
    }
    assert.equal(check({ SHOULD_RUN: 'false', SNIPPETS_RESULT: 'skipped' }), 0);
    assert.equal(
      check({ CAN_USE_SECRETS: 'false', PRODUCTION_PATH_RESULT: 'skipped' }),
      0,
    );
    assert.equal(
      check({ CAN_USE_SECRETS: 'true', PRODUCTION_PATH_RESULT: 'skipped' }),
      1,
    );
  }
});

test('merge-group change detection checks the diff and fails if it cannot read it', () => {
  const step = workflow.jobs['determine-changes'].steps.find(
    (item) => item.id === 'changes',
  );
  assert.ok(step?.run);
  assert.match(step.env?.BASE_SHA ?? '', /merge_group.base_sha/);
  assert.match(step.env?.HEAD_SHA ?? '', /merge_group.head_sha/);
  const root = mkdtempSync(join(tmpdir(), 'docs-workflow-'));
  const git = (...args: string[]) =>
    execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  try {
    git('init', '-q');
    git('config', 'user.name', 'Docs test');
    git('config', 'user.email', 'docs-test@example.invalid');
    git('commit', '--allow-empty', '-qm', 'base');
    const base = git('rev-parse', 'HEAD');
    writeFileSync(join(root, 'unrelated.txt'), 'unrelated');
    git('add', '.');
    git('commit', '-qm', 'unrelated');
    const unrelated = git('rev-parse', 'HEAD');
    const output = join(root, 'outputs');
    const run = (baseSha: string, headSha: string) => {
      writeFileSync(output, '');
      const result = spawnSync('bash', ['-c', step.run ?? ''], {
        cwd: root,
        encoding: 'utf8',
        env: {
          ...process.env,
          BASE_SHA: baseSha,
          EVENT_NAME: 'merge_group',
          GITHUB_OUTPUT: output,
          HEAD_SHA: headSha,
        },
      });
      return { output: readFileSync(output, 'utf8'), status: result.status };
    };
    assert.deepEqual(run(base, unrelated), {
      output: 'can-use-secrets=true\nshould-run=false\n',
      status: 0,
    });
    writeFileSync(join(root, 'mise.toml'), '# changed runtime input');
    git('add', 'mise.toml');
    git('commit', '-qm', 'runtime input');
    assert.deepEqual(run(base, git('rev-parse', 'HEAD')), {
      output: 'can-use-secrets=true\nshould-run=true\n',
      status: 0,
    });
    assert.notEqual(run('missing-base', unrelated).status, 0);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});
