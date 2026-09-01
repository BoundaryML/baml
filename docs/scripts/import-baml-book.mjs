#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { access, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { parse as parseToml } from 'smol-toml';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const defaultManifestPath = path.join(packageRoot, 'book-import.json');
const outputRoot = path.join(packageRoot, 'content', 'baml', 'book');
const generatedManifestPath = path.join(packageRoot, 'generated', 'book', 'manifest.json');
const execFileAsync = promisify(execFile);

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function json(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function within(root, target, label) {
  const resolved = path.resolve(root, target);
  if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
    throw new Error(`${label} escapes the book repository: ${target}`);
  }
  return resolved;
}

async function exists(target) {
  try {
    await access(target);
    return true;
  } catch {
    return false;
  }
}

function parseAttributes(raw) {
  const attributes = {};
  for (const match of raw.matchAll(/([a-z][a-z-]*)\s*=\s*"([^"]*)"/gi)) {
    attributes[match[1]] = match[2];
  }
  return attributes;
}

function inlineMarkdownToJsx(value) {
  const escaped = value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
  return escaped
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>');
}

function extractAnchor(source, anchor, sourcePath) {
  if (!anchor) {
    return source
      .split('\n')
      .filter((line) => !/^\s*\/\/\s*ANCHOR(?:_END)?:\s*/.test(line))
      .join('\n');
  }

  const lines = source.split('\n');
  const start = lines.findIndex((line) =>
    new RegExp(`^\\s*//\\s*ANCHOR:\\s*${anchor.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`).test(line),
  );
  const end = lines.findIndex((line, index) =>
    index > start && new RegExp(`^\\s*//\\s*ANCHOR_END:\\s*${anchor.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`).test(line),
  );
  if (start === -1 || end === -1) {
    throw new Error(`Missing anchor '${anchor}' in ${sourcePath}`);
  }
  return lines
    .slice(start + 1, end)
    .filter((line) => !/^\s*\/\/\s*ANCHOR(?:_END)?:\s*/.test(line))
    .join('\n');
}

async function expandIncludes(markdown, chapterPath, sourceRoot) {
  const includes = [...markdown.matchAll(/\{\{#include\s+([^}\s]+)\s*\}\}/g)];
  let output = markdown;
  for (const match of includes) {
    const specifier = match[1];
    const anchorMatch = specifier.match(/^(.*):([A-Za-z0-9_-]+)$/);
    const relativePath = anchorMatch ? anchorMatch[1] : specifier;
    const anchor = anchorMatch?.[2];
    const includePath = within(
      sourceRoot,
      path.relative(sourceRoot, path.resolve(path.dirname(chapterPath), relativePath)),
      'Include path',
    );
    const included = await readFile(includePath, 'utf8');
    output = output.replace(match[0], extractAnchor(included, anchor, includePath));
  }
  return output;
}

async function readRunnableProject(sourceRoot, projectName) {
  if (!projectName || path.isAbsolute(projectName) || projectName.split('/').includes('..')) {
    throw new Error(`Invalid runnable listing path: ${projectName}`);
  }
  const projectRoot = within(sourceRoot, path.join('listings', projectName), 'Runnable listing path');
  const files = {
    'baml.toml': await readFile(path.join(projectRoot, 'baml.toml'), 'utf8'),
  };
  const sourceDir = path.join(projectRoot, 'baml_src');
  for (const entry of (await readdir(sourceDir, { withFileTypes: true }))
    .filter((entry) => entry.isFile() && entry.name.endsWith('.baml'))
    .sort((a, b) => a.name.localeCompare(b.name))) {
    files[`baml_src/${entry.name}`] = await readFile(path.join(sourceDir, entry.name), 'utf8');
  }
  if (Object.keys(files).length === 1) {
    throw new Error(`Runnable listing '${projectName}' has no baml_src/*.baml files`);
  }
  return files;
}

async function convertListings(markdown, sourceRoot) {
  const pattern = /<Listing\b([^>]*)>([\s\S]*?)<\/Listing>/g;
  const matches = [...markdown.matchAll(pattern)];
  let output = markdown;
  for (const match of matches) {
    const attributes = parseAttributes(match[1]);
    const props = [];
    if (attributes.number) props.push(`number=${JSON.stringify(attributes.number)}`);
    if (attributes['file-name']) props.push(`fileName=${JSON.stringify(attributes['file-name'])}`);
    if (attributes.caption) props.push(`caption={<>${inlineMarkdownToJsx(attributes.caption)}</>}`);
    let body = match[2].trim();
    if (attributes.runnable) {
      const files = await readRunnableProject(sourceRoot, attributes.runnable);
      body += `\n\n<BamlRunner files={${JSON.stringify(files)}} showSource={false} />`;
    }
    output = output.replace(match[0], `<BookListing ${props.join(' ')}>\n\n${body}\n\n</BookListing>`);
  }
  return output;
}

async function convertQuizzes(markdown, chapterPath, sourceRoot) {
  const includes = [...markdown.matchAll(/\{\{#quiz\s+([^}\s]+)\s*\}\}/g)];
  let output = markdown;
  for (const match of includes) {
    const quizPath = within(
      sourceRoot,
      path.relative(sourceRoot, path.resolve(path.dirname(chapterPath), match[1])),
      'Quiz path',
    );
    const parsed = parseToml(await readFile(quizPath, 'utf8'));
    if (!Array.isArray(parsed.questions) || parsed.questions.length === 0) {
      throw new Error(`Quiz has no questions: ${quizPath}`);
    }
    output = output.replace(match[0], `<BookQuiz questions={${JSON.stringify(parsed.questions)}} />`);
  }
  return output;
}

function convertNotes(markdown) {
  return markdown.replace(/^> Note:\s*(.*(?:\n>.*)*)/gm, (_match, quoted) => {
    const body = quoted.replace(/^>\s?/gm, '');
    return `<Callout title="Note">\n\n${body}\n\n</Callout>`;
  });
}

export async function convertChapter({ chapter, sourceRoot }) {
  const chapterPath = within(sourceRoot, chapter.source, 'Chapter source');
  const raw = await readFile(chapterPath, 'utf8');
  const actualSourceSha256 = sha256(raw);
  if (actualSourceSha256 !== chapter.sourceSha256) {
    throw new Error(
      `Approval hash mismatch for ${chapter.source}: expected ${chapter.sourceSha256}, got ${actualSourceSha256}`,
    );
  }

  const heading = raw.match(/^#\s+(.+)$/m)?.[1];
  const title = chapter.title ?? heading;
  if (!title) throw new Error(`Chapter needs a title: ${chapter.source}`);

  let body = raw.replace(/^#\s+.+\n+/, '');
  body = await expandIncludes(body, chapterPath, sourceRoot);
  body = await convertListings(body, sourceRoot);
  body = await convertQuizzes(body, chapterPath, sourceRoot);
  body = convertNotes(body).trim();

  const description = chapter.description ?? `Read ${title} in The BAML Programming Language.`;
  const content = [
    '---',
    `title: ${JSON.stringify(title)}`,
    `description: ${JSON.stringify(description)}`,
    '---',
    '',
    body,
    '',
  ].join('\n');
  return { content, sourceSha256: actualSourceSha256 };
}

function validateManifest(manifest) {
  if (manifest.schemaVersion !== 1) throw new Error('book-import.json schemaVersion must be 1');
  if (!manifest.source?.repository || !manifest.source?.revision) {
    throw new Error('book-import.json must pin a source repository and revision');
  }
  if (!Array.isArray(manifest.chapters)) throw new Error('book-import.json chapters must be an array');
  const outputs = new Set();
  for (const chapter of manifest.chapters) {
    if (chapter.status !== 'approved') {
      throw new Error(`${chapter.source}: only explicitly approved chapters may enter the import manifest`);
    }
    if (!chapter.source || !chapter.output || !/^[0-9a-f]{64}$/.test(chapter.sourceSha256 ?? '')) {
      throw new Error('Every approved chapter needs source, output, and sourceSha256');
    }
    if (!/^[a-z0-9][a-z0-9/_-]*\.mdx$/.test(chapter.output) || chapter.output === 'index.mdx') {
      throw new Error(`Invalid managed chapter output: ${chapter.output}`);
    }
    if (outputs.has(chapter.output)) throw new Error(`Duplicate chapter output: ${chapter.output}`);
    outputs.add(chapter.output);
  }
}

async function readApprovalManifest(manifestPath) {
  const raw = await readFile(manifestPath, 'utf8');
  const manifest = JSON.parse(raw);
  validateManifest(manifest);
  return { manifest, sha256: sha256(raw) };
}

function navigationFor(chapters) {
  return {
    title: 'The BAML Programming Language',
    pages: ['index', ...chapters.map((chapter) => chapter.output.replace(/\.mdx$/, ''))],
  };
}

async function verifySourceCheckout(sourceRoot, revision) {
  const [{ stdout: head }, { stdout: status }] = await Promise.all([
    execFileAsync('git', ['-C', sourceRoot, 'rev-parse', 'HEAD']),
    execFileAsync('git', ['-C', sourceRoot, 'status', '--porcelain', '--untracked-files=all']),
  ]);
  if (head.trim() !== revision) {
    throw new Error(`baml-book checkout is at ${head.trim()}, but book-import.json pins ${revision}`);
  }
  if (status.trim()) {
    throw new Error('baml-book checkout has uncommitted files; import from a clean checkout of the pinned revision');
  }
}

async function buildDesired(manifest, sourceRoot) {
  if (manifest.chapters.length > 0 && !sourceRoot) {
    throw new Error('Pass --source <baml-book checkout> to import approved chapters');
  }
  if (manifest.chapters.length > 0) {
    await verifySourceCheckout(sourceRoot, manifest.source.revision);
  }
  const files = [];
  for (const chapter of manifest.chapters) {
    const converted = await convertChapter({ chapter, sourceRoot });
    files.push({
      output: chapter.output,
      outputSha256: sha256(converted.content),
      source: chapter.source,
      sourceSha256: converted.sourceSha256,
      content: converted.content,
    });
  }
  return files;
}

async function listManagedPages(directory, prefix = '') {
  if (!(await exists(directory))) return [];
  const pages = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = path.posix.join(prefix, entry.name);
    if (entry.isDirectory()) pages.push(...await listManagedPages(path.join(directory, entry.name), relative));
    else if (entry.isFile() && entry.name.endsWith('.mdx') && relative !== 'index.mdx') pages.push(relative);
  }
  return pages.sort();
}

async function verifyCurrent({ approvalSha256, manifest }) {
  const generated = JSON.parse(await readFile(generatedManifestPath, 'utf8'));
  if (generated.approvalManifestSha256 !== approvalSha256) {
    throw new Error('Book approval manifest changed; run pnpm generate:book with the approved baml-book checkout');
  }
  const expectedOutputs = manifest.chapters.map((chapter) => chapter.output).sort();
  const generatedOutputs = generated.files.map((file) => file.output).sort();
  if (JSON.stringify(expectedOutputs) !== JSON.stringify(generatedOutputs)) {
    throw new Error('Generated book manifest does not match approved chapter outputs');
  }
  for (const file of generated.files) {
    const content = await readFile(within(outputRoot, file.output, 'Generated output'), 'utf8');
    if (sha256(content) !== file.outputSha256) throw new Error(`Generated book page drifted: ${file.output}`);
  }
  const currentPages = await listManagedPages(outputRoot);
  if (JSON.stringify(currentPages) !== JSON.stringify(expectedOutputs)) {
    throw new Error(`Unmanaged book pages found; expected ${expectedOutputs.join(', ') || 'none'}, got ${currentPages.join(', ') || 'none'}`);
  }
  const meta = JSON.parse(await readFile(path.join(outputRoot, 'meta.json'), 'utf8'));
  if (JSON.stringify(meta) !== JSON.stringify(navigationFor(manifest.chapters))) {
    throw new Error('Book navigation drifted from the approval manifest');
  }
  console.log(`Book import is current (${generated.files.length} approved chapters).`);
}

async function writeGenerated({ approvalSha256, files, manifest }) {
  let previous = { files: [] };
  if (await exists(generatedManifestPath)) {
    previous = JSON.parse(await readFile(generatedManifestPath, 'utf8'));
  }
  const desired = new Set(files.map((file) => file.output));
  for (const file of previous.files ?? []) {
    if (!desired.has(file.output)) await rm(within(outputRoot, file.output, 'Stale generated output'));
  }
  for (const file of files) {
    const target = within(outputRoot, file.output, 'Generated output');
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, file.content);
  }
  await mkdir(path.dirname(generatedManifestPath), { recursive: true });
  await writeFile(path.join(outputRoot, 'meta.json'), json(navigationFor(manifest.chapters)));
  await writeFile(generatedManifestPath, json({
    schemaVersion: 1,
    source: manifest.source,
    approvalManifestSha256: approvalSha256,
    files: files.map(({ content: _content, ...file }) => file),
  }));
  console.log(`Imported ${files.length} approved book chapters.`);
}

async function main() {
  const args = process.argv.slice(2);
  const check = args.includes('--check');
  const sourceIndex = args.indexOf('--source');
  const manifestIndex = args.indexOf('--manifest');
  const sourceArgument = sourceIndex === -1 ? process.env.BAML_BOOK_SOURCE : args[sourceIndex + 1];
  const sourceRoot = sourceArgument ? path.resolve(sourceArgument) : undefined;
  const manifestPath = manifestIndex === -1 ? defaultManifestPath : path.resolve(args[manifestIndex + 1]);
  const approval = await readApprovalManifest(manifestPath);

  if (check && !sourceRoot) {
    await verifyCurrent({ approvalSha256: approval.sha256, manifest: approval.manifest });
    return;
  }
  const files = await buildDesired(approval.manifest, sourceRoot);
  if (check) {
    await verifyCurrent({ approvalSha256: approval.sha256, manifest: approval.manifest });
    for (const desired of files) {
      const current = await readFile(within(outputRoot, desired.output, 'Generated output'), 'utf8');
      if (current !== desired.content) throw new Error(`Generated book page is stale: ${desired.output}`);
    }
    return;
  }
  await writeGenerated({ approvalSha256: approval.sha256, files, manifest: approval.manifest });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
