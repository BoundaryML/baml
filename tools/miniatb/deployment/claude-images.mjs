import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export function multimodalInput(prompt, images) {
  return JSON.stringify({ type: 'user', message: { role: 'user', content: [
    { type: 'text', text: prompt }, ...images,
  ] } }) + '\n';
}
async function main() {
  const id = process.env.MINIATB_RUN;
  const dataset = process.env.MINIATB_DATASET ?? 'eval';
  if (!/^[a-f0-9]{24}$/.test(id ?? '') || !['eval', 'live'].includes(dataset)) throw new Error('Invalid run identity');
  const path = `${process.env.MINIATB_DATA ?? '/data/miniatb'}/${dataset}/runs/${id}/slack-context.json`;
  let images = [];
  try { images = JSON.parse(await readFile(path, 'utf8')).images; }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const args = process.argv.slice(2);
  if (images.length) args.push('--input-format', 'stream-json');
  const child = spawn('setpriv', ['--reuid=1000', '--regid=1000', '--clear-groups',
    '/usr/local/bin/claude', '--strict-mcp-config', '--setting-sources', '', '--disable-slash-commands', ...args], {
    cwd: '/app', env: { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: process.env.MINIATB_CLAUDE_HOME ?? '/data/claude',
      CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1' }, stdio: ['pipe', 'inherit', 'inherit'],
  });
  for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal));
  child.on('error', () => { console.error('Claude launcher failed'); process.exitCode = 1; });
  child.on('exit', code => { process.exitCode = code ?? 1; });
  child.stdin.on('error', () => {});
  if (images.length) {
    const chunks = [];
    for await (const chunk of process.stdin) chunks.push(chunk);
    child.stdin.end(multimodalInput(Buffer.concat(chunks).toString(), images));
  } else process.stdin.pipe(child.stdin);
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(() => { console.error('Claude image transport failed'); process.exitCode = 1; });
}
