import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resources } from './sandbox-run.mjs';
const enabled = process.env.MINIATB_LINUX_TEST === '1';
test('kernel enforces aggregate memory and process limits and kills background descendants', { skip: !enabled }, () => {
  const r = resources('/sys/fs/cgroup', 128 * 1024 ** 2, 16);
  try {
    const join = 'for group in "$@"; do [ "$group" = -- ] && break; echo $$ > "$group/cgroup.procs" || exit 125; shift; done; shift; exec "$@"';
    const run = args => spawnSync('sh', ['-c', join, 'limits', ...r.groups, '--', ...args], { encoding: 'utf8', timeout: 15000 });
    const memory = run(['node', '-e', 'const keep=[];setInterval(()=>keep.push(Buffer.alloc(16*1024*1024,1)),5)']);
    assert.ok(memory.signal || memory.status !== 0, 'memory allocation must be killed');
    assert.equal(memory.error, undefined, 'kernel must enforce memory before the watchdog');
    const pids = run(['node', '-e', 'const{spawn}=require("child_process");let errors=0;for(let i=0;i<50;i++){const p=spawn("sleep",["3"]);p.on("error",()=>errors++);}setTimeout(()=>{console.log(errors);process.exit(errors>0?0:1)},500)']);
    assert.equal(pids.status, 0, pids.stderr);
    assert.ok(Number(pids.stdout.trim()) > 0);
  } finally { r.cleanup(); }
  for (const group of r.groups) assert.equal(fs.existsSync(group), false);
});
