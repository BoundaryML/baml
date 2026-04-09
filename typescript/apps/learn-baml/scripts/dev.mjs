import { spawn } from 'child_process';

const pnpmCmd = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';

const commands = [
  { name: 'site', cmd: pnpmCmd, args: ['dev:site'] },
  { name: 'api', cmd: pnpmCmd, args: ['dev:api'] },
];

const children = commands.map(({ cmd, args }) =>
  spawn(cmd, args, { stdio: 'inherit', env: process.env })
);

let shuttingDown = false;
const shutdown = () => {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const child of children) {
    if (!child.killed) {
      child.kill('SIGTERM');
    }
  }
};

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

children.forEach(child => {
  child.on('exit', code => {
    shutdown();
    process.exit(code ?? 0);
  });
});
