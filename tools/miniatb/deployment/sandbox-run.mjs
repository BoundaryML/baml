import fs from 'node:fs';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const run = (cmd, args) => execFileSync(cmd, args, { stdio: ['ignore', 'ignore', 'pipe'] });
const write = (file, value) => fs.writeFileSync(file, String(value));

// One cgroup contains the command and every descendant, including background jobs.
export function resources(root = '/sys/fs/cgroup', memory = 12 * 1024 ** 3, pids = 128) {
  const name = `miniatb-${process.pid}-${Date.now()}`;
  const groups = [];
  const v2 = fs.existsSync(`${root}/cgroup.controllers`);
  function group(parent) {
    const dir = `${parent}/${name}`;
    fs.mkdirSync(dir);
    groups.push(dir);
    return dir;
  }
  function cleanup() {
    for (const dir of groups) {
      if (fs.existsSync(`${dir}/cgroup.kill`)) write(`${dir}/cgroup.kill`, 1);
      else for (const pid of fs.readFileSync(`${dir}/cgroup.procs`, 'utf8').trim().split('\n')) {
        if (pid) try { process.kill(Number(pid), 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') throw e; }
      }
    }
    for (const dir of groups.reverse()) {
      // Killed descendants may take a moment to be reaped.
      for (let n = 0; ; n++) {
        try { fs.rmdirSync(dir); break; }
        catch (e) { if (e.code !== 'EBUSY' || n === 100) throw e; Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10); }
      }
    }
  }
  try {
    if (v2) {
      const available = fs.readFileSync(`${root}/cgroup.controllers`, 'utf8').split(/\s+/);
      if (!['memory', 'pids', 'cpu'].every(c => available.includes(c))) throw new Error('Required cgroup controllers unavailable');
      write(`${root}/cgroup.subtree_control`, '+memory +pids +cpu');
      const dir = group(root);
      write(`${dir}/memory.max`, memory);
      write(`${dir}/memory.swap.max`, 0);
      write(`${dir}/memory.oom.group`, 1);
      write(`${dir}/pids.max`, pids);
      write(`${dir}/cpu.max`, '200000 100000');
    } else {
      const mem = group(`${root}/memory`);
      write(`${mem}/memory.limit_in_bytes`, memory);
      if (fs.existsSync(`${mem}/memory.memsw.limit_in_bytes`)) write(`${mem}/memory.memsw.limit_in_bytes`, memory);
      write(`${group(`${root}/pids`)}/pids.max`, pids);
      const cpuRoot = fs.existsSync(`${root}/cpu`) ? `${root}/cpu` : `${root}/cpu,cpuacct`;
      const cpu = group(cpuRoot);
      write(`${cpu}/cpu.cfs_period_us`, 100000);
      write(`${cpu}/cpu.cfs_quota_us`, 200000);
    }
    return { groups, cleanup };
  } catch (e) {
    cleanup();
    throw e;
  }
}

export function boundedTree(root, maxBytes, maxEntries = 100000) {
  let bytes = 0, entries = 0;
  function visit(dir) {
    for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
      if (++entries > maxEntries) throw new Error('Workspace entry limit exceeded');
      const file = path.join(dir, item.name);
      const st = fs.lstatSync(file);
      if (st.isFile()) bytes += st.size;
      else if (st.isDirectory()) visit(file);
      else if (!st.isSymbolicLink()) throw new Error('Unsupported workspace artifact');
      if (bytes > maxBytes) throw new Error('Workspace storage limit exceeded');
    }
  }
  visit(root);
}

// The caller enters a private mount namespace. Only bounded, completed writes persist.
export function sandboxRun(source, scratch, mode, seconds, args) {
  if (![0, 1, 2].includes(mode)) throw new Error('Invalid source mode');
  const writable = mode !== 0;
  if (![300, 1800].includes(seconds)) throw new Error("Invalid command deadline");
  for (const dir of [source, scratch]) {
    if (!path.isAbsolute(dir) || fs.lstatSync(dir).isSymbolicLink() || /[,:\n]/.test(dir)) throw new Error('Invalid sandbox directory');
  }
  const temp = fs.mkdtempSync('/tmp/miniatb-sandbox-');
  let quota;
  try {
    run('mount', ['-t', 'tmpfs', '-o', 'size=8G,nr_inodes=131072,mode=0700', 'tmpfs', temp]);
    quota = resources();
    for (const dir of ['scratch', 'upper', 'work', 'source']) fs.mkdirSync(`${temp}/${dir}`, { mode: 0o777 });
    boundedTree(scratch, 4 * 1024 ** 3);
    run('rsync', ['-a', '--safe-links', `${scratch}/`, `${temp}/scratch/`]);
    let tree = source;
    if (writable) {
      boundedTree(source, 2 * 1024 ** 3);
      run('mount', ['-t', 'overlay', 'overlay', '-o', `lowerdir=${source},upperdir=${temp}/upper,workdir=${temp}/work`, `${temp}/source`]);
      tree = `${temp}/source`;
    }
    const binds = new Set(['--bind', '--ro-bind']);
    args = args.map((arg, i) => binds.has(args[i - 1]) ? arg === source ? tree : arg === scratch ? `${temp}/scratch` : arg : arg);
    const join = 'for group in "$@"; do [ "$group" = -- ] && break; printf "%s\\n" "$$" > "$group/cgroup.procs" || exit 125; shift; done; shift; exec "$@"';
    const result = spawnSync('/bin/sh', ['-c', join, 'sandbox', ...quota.groups, '--', 'prlimit', '--fsize=1073741824:1073741824', '--nofile=1024:1024', '--', 'timeout', '--kill-after=5s', `${seconds}s`, 'bwrap', ...args], { stdio: 'inherit', env: { PATH: '/usr/local/bin:/usr/bin:/bin' }, timeout: (seconds + 10) * 1000 });
    quota.cleanup(); quota = null;
    // A timeout, OOM, or setup failure must not persist partial changes.
    if (result.error || result.signal || result.status === null || result.status >= 124) return 125;
    boundedTree(`${temp}/scratch`, 4 * 1024 ** 3);
    if (mode === 1) {
      boundedTree(tree, 2 * 1024 ** 3);
      run('rsync', ['-a', '--checksum', '--delete', '--safe-links', '--exclude=/.git', `${tree}/`, `${source}/`]);
    }
    run('rsync', ['-a', '--checksum', '--delete', '--safe-links', `${temp}/scratch/`, `${scratch}/`]);
    return result.status;
  } finally {
    if (quota) quota.cleanup();
    // All mounts disappear with this process's mount namespace, even on SIGKILL.
    try { run('umount', ['-R', temp]); } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { process.exitCode = sandboxRun(process.argv[2], process.argv[3], Number(process.argv[4]), Number(process.argv[5]), process.argv.slice(6)); }
  catch { process.stderr.write('Sandbox resource or storage boundary failed; refusing execution.\n'); process.exitCode = 125; }
}
