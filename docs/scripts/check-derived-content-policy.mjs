#!/usr/bin/env node

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { run } from './generated-content.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repositoryRoot = path.resolve(packageRoot, '..');
const generatedPaths = [
  'docs/content/baml/language/reference',
  'docs/content/cli/commands',
  'docs/generated/baml',
  'docs/generated/cli',
  'docs/generated/book',
  'docs/content/baml/book/meta.json',
];

const trackedGenerated = run('git', ['ls-files', '--', ...generatedPaths], { cwd: repositoryRoot });
const trackedBookPages = run('git', ['ls-files', '--', 'docs/content/baml/book'], { cwd: repositoryRoot })
  .split('\n')
  .filter((file) => file && file !== 'docs/content/baml/book/index.mdx' && file.endsWith('.mdx'));
const tracked = [...trackedGenerated.split('\n').filter(Boolean), ...trackedBookPages];
if (tracked.length > 0) {
  console.error('Derived documentation must be generated during the build, not tracked by git:');
  for (const file of tracked.slice(0, 30)) console.error(`- ${file}`);
  const remaining = tracked.length - 30;
  if (remaining > 0) console.error(`- …and ${remaining} more`);
  process.exitCode = 1;
} else {
  console.log('Generated language, CLI, and book content is not tracked by git.');
}
