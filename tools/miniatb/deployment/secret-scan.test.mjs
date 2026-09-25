import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import { randomInt } from 'node:crypto';
import { spawnSync } from 'node:child_process';

const enabled = fs.existsSync('/opt/baml/bin/baml');
test('publisher ignores agent scan configuration and detects a synthetic credential', { skip: !enabled }, () => {
  const dir = fs.mkdtempSync(os.tmpdir() + '/miniatb-scan-');
  const scan = () => spawnSync('/opt/baml/bin/baml', [
    'run', '--directory', '/app', '--agent-skill-check', 'off', 'scan_worktree', '--', '--tree', dir,
  ], { encoding: 'utf8', timeout: 60000 });
  try {
    fs.writeFileSync(dir + '/source.txt', 'ordinary source text\n');
    const clean = scan();
    assert.equal(clean.status, 0, clean.stderr);
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    const fake = 'gh' + 'p_' + Array.from({ length: 36 }, () => alphabet[randomInt(alphabet.length)]).join('');
    fs.writeFileSync(dir + '/source.txt', `token = "${fake}"\n`);
    fs.writeFileSync(dir + '/.infisical-scan.toml', '[extend]\nuseDefault = true\n[allowlist]\nregexes = [".*"]\n');
    const insecure = spawnSync('infisical', ['scan', '--no-git', '--source', dir, '--redact', '--telemetry=false'], { encoding: 'utf8', timeout: 30000 });
    assert.equal(insecure.status, 0, 'fixture must reproduce the implicit-config bypass');
    const protectedScan = scan();
    assert.notEqual(protectedScan.status, 0);
    assert.match(protectedScan.stderr, /leaks found/);
    assert.ok(!protectedScan.stdout.includes(fake) && !protectedScan.stderr.includes(fake), 'diagnostics must redact credentials');
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
