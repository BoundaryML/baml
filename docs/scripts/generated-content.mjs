import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

export function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = result.stderr || result.stdout || `process exited with ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed:\n${detail}`);
  }
  return (result.stdout ?? '').trim();
}

export async function readGeneratedTree(root) {
  const files = new Map();
  async function visit(directory, prefix = '') {
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch (error) {
      if (error.code === 'ENOENT') return;
      throw error;
    }
    for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
      const relative = path.posix.join(prefix, entry.name);
      if (entry.isDirectory()) await visit(path.join(directory, entry.name), relative);
      else if (entry.isFile()) files.set(relative, await readFile(path.join(directory, entry.name), 'utf8'));
    }
  }
  await visit(root);
  return files;
}

export function diffGeneratedTrees(expected, actual) {
  const names = new Set([...expected.keys(), ...actual.keys()]);
  return [...names].sort().filter((name) => expected.get(name) !== actual.get(name));
}

function generatedDestination(root, relative) {
  if (path.isAbsolute(relative) || relative.split('/').includes('..')) {
    throw new Error(`Unsafe generated path: ${JSON.stringify(relative)}`);
  }
  return path.join(root, ...relative.split('/'));
}

export async function writeGeneratedTree(root, files) {
  const parent = path.dirname(root);
  await mkdir(parent, { recursive: true });
  const temporary = await mkdtemp(path.join(parent, `.${path.basename(root)}-`));
  try {
    for (const [relative, contents] of [...files].sort(([a], [b]) => a.localeCompare(b))) {
      const destination = generatedDestination(temporary, relative);
      await mkdir(path.dirname(destination), { recursive: true });
      await writeFile(destination, contents);
    }
    await rm(root, { recursive: true, force: true });
    await rename(temporary, root);
  } catch (error) {
    await rm(temporary, { recursive: true, force: true });
    throw error;
  }
}

export async function checkGeneratedTree(root, expected, label) {
  const changed = diffGeneratedTrees(expected, await readGeneratedTree(root));
  if (changed.length === 0) return [];
  return changed.map((name) => `${label}/${name}`);
}
