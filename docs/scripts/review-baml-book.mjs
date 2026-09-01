#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { convertChapter, verifySourceCheckout } from './import-baml-book.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const defaultManifestPath = path.join(packageRoot, 'book-import.json');

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function json(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function slugify(value) {
  return value
    .toLowerCase()
    .replace(/^ch\d+(?:-\d+)?-/, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

export function parseSummary(summary) {
  const entries = [];
  for (const line of summary.split('\n')) {
    const match = line.match(/^\s*(?:[-*+]\s+)?\[([^\]]+)]\(([^)]+\.md)(?:#[^)]+)?\)\s*$/);
    if (!match) continue;
    const [, title, source] = match;
    if (path.posix.isAbsolute(source) || source.split('/').includes('..')) {
      throw new Error(`Book summary entry escapes src/: ${source}`);
    }
    const basename = path.posix.basename(source, '.md');
    const slug = slugify(basename) || slugify(title);
    if (!slug) throw new Error(`Cannot derive an output route for ${source}`);
    entries.push({ title, source: path.posix.join('src', source), output: `${slug}.mdx` });
  }
  if (entries.length === 0) throw new Error('src/SUMMARY.md contains no Markdown chapters');
  const duplicate = entries.find((entry, index) => entries.findIndex((candidate) => candidate.output === entry.output) !== index);
  if (duplicate) throw new Error(`Book summary produces duplicate output: ${duplicate.output}`);
  return entries;
}

function sourceStats(raw) {
  return {
    lines: raw === '' ? 0 : raw.split('\n').length,
    words: raw.trim() === '' ? 0 : raw.trim().split(/\s+/).length,
    includes: [...raw.matchAll(/\{\{#include\s+/g)].length,
    listings: [...raw.matchAll(/<Listing\b/g)].length,
    runnableListings: [...raw.matchAll(/<Listing\b[^>]*\brunnable=/g)].length,
    quizzes: [...raw.matchAll(/\{\{#quiz\s+/g)].length,
  };
}

export async function buildReviewBundle({ manifestPath = defaultManifestPath, sourceRoot }) {
  if (!sourceRoot) throw new Error('Pass --source <clean baml-book checkout>');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  if (manifest.schemaVersion !== 1 || !manifest.source?.repository || !manifest.source?.revision) {
    throw new Error('book-import.json must contain a versioned source repository and revision');
  }
  await verifySourceCheckout(sourceRoot, manifest.source.revision);
  const summary = await readFile(path.join(sourceRoot, 'src', 'SUMMARY.md'), 'utf8');
  const chapters = parseSummary(summary);
  const candidates = [];
  for (const chapter of chapters) {
    const raw = await readFile(path.join(sourceRoot, chapter.source), 'utf8');
    const sourceSha256 = sha256(raw);
    const approvalEntry = {
      source: chapter.source,
      output: chapter.output,
      status: 'approved',
      sourceSha256,
    };
    const converted = await convertChapter({ chapter: { ...approvalEntry, title: chapter.title }, sourceRoot });
    candidates.push({
      title: chapter.title,
      source: chapter.source,
      proposedOutput: chapter.output,
      sourceSha256,
      convertedSha256: sha256(converted.content),
      stats: sourceStats(raw),
      approvalEntry,
      convertedContent: converted.content,
    });
  }
  return {
    schemaVersion: 1,
    disposition: 'candidate-only',
    source: manifest.source,
    candidates,
  };
}

function reviewMarkdown(bundle) {
  const lines = [
    '# BAML book editorial review bundle',
    '',
    '> None of these chapters is approved or published. Audit the source and converted MDX before copying an approval entry into `docs/book-import.json`.',
    '',
    `Source revision: \`${bundle.source.revision}\``,
    '',
  ];
  for (const candidate of bundle.candidates) {
    lines.push(
      `## ${candidate.title}`,
      '',
      `- Source: \`${candidate.source}\``,
      `- Proposed route: \`/baml/book/${candidate.proposedOutput.replace(/\.mdx$/, '')}\``,
      `- Source SHA-256: \`${candidate.sourceSha256}\``,
      `- Converted SHA-256: \`${candidate.convertedSha256}\``,
      `- Scope: ${candidate.stats.words} words, ${candidate.stats.listings} listings (${candidate.stats.runnableListings} runnable), ${candidate.stats.quizzes} quizzes, ${candidate.stats.includes} includes`,
      '',
      'Approval entry to use only after editorial sign-off:',
      '',
      '```json',
      JSON.stringify(candidate.approvalEntry, null, 2),
      '```',
      '',
      `Converted preview: [${candidate.proposedOutput}](./converted/${candidate.proposedOutput})`,
      '',
    );
  }
  return `${lines.join('\n')}\n`;
}

export async function writeReviewBundle({ bundle, outputRoot }) {
  await rm(outputRoot, { recursive: true, force: true });
  await mkdir(path.join(outputRoot, 'converted'), { recursive: true });
  for (const candidate of bundle.candidates) {
    await writeFile(path.join(outputRoot, 'converted', candidate.proposedOutput), candidate.convertedContent);
  }
  const serializable = {
    ...bundle,
    candidates: bundle.candidates.map(({ convertedContent: _convertedContent, ...candidate }) => candidate),
  };
  await writeFile(path.join(outputRoot, 'review.json'), json(serializable));
  await writeFile(path.join(outputRoot, 'README.md'), reviewMarkdown(bundle));
}

async function main() {
  const args = process.argv.slice(2);
  const sourceIndex = args.indexOf('--source');
  const outputIndex = args.indexOf('--output');
  const manifestIndex = args.indexOf('--manifest');
  if (sourceIndex === -1 || !args[sourceIndex + 1]) throw new Error('Pass --source <clean baml-book checkout>');
  if (outputIndex === -1 || !args[outputIndex + 1]) throw new Error('Pass --output <review bundle directory>');
  const bundle = await buildReviewBundle({
    sourceRoot: path.resolve(args[sourceIndex + 1]),
    manifestPath: manifestIndex === -1 ? defaultManifestPath : path.resolve(args[manifestIndex + 1]),
  });
  const outputRoot = path.resolve(args[outputIndex + 1]);
  await writeReviewBundle({ bundle, outputRoot });
  console.log(`Prepared ${bundle.candidates.length} unapproved chapter candidates at ${outputRoot}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
