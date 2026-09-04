import { spawn } from 'node:child_process';
import { copyFile, mkdtemp, readdir, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, relative, resolve, sep } from 'node:path';

import {
  discoverProjectSnippets,
  discoverStandaloneSnippets,
  type ProjectSnippet,
  type StandaloneSnippet,
} from './discovery';
import type { ExpectedDiagnostic, SnippetExpectation } from './schema';

export interface CompilerDiagnostic {
  code: string;
  message: string;
  output: string;
}

export interface CompilerCheckResult {
  diagnostics: CompilerDiagnostic[];
  exitCode: number | null;
  output: string;
}

export interface SnippetValidationResult {
  id: string;
  kind: 'project' | 'standalone';
  pageSources: string[];
  sourcePath: string;
}

const diagnosticStartPattern = /^.*? error\[([^\]]+)\]: (.*)$/gm;

function runProcess(
  binary: string,
  argumentsToPass: readonly string[],
): Promise<{
  exitCode: number | null;
  output: string;
}> {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(binary, argumentsToPass, {
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let output = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => {
      output += chunk;
    });
    child.stderr.on('data', (chunk: string) => {
      output += chunk;
    });
    child.on('error', rejectPromise);
    child.on('close', (exitCode) => resolvePromise({ exitCode, output }));
  });
}

export function parseCompilerDiagnostics(output: string): CompilerDiagnostic[] {
  const matches = [...output.matchAll(diagnosticStartPattern)];
  return matches.map((match, index) => {
    const start = match.index ?? 0;
    const end = matches[index + 1]?.index ?? output.length;
    return {
      code: match[1],
      message: match[2],
      output: output.slice(start, end).trim(),
    };
  });
}

async function checkProject(
  binary: string,
  projectPath: string,
): Promise<CompilerCheckResult> {
  const result = await runProcess(binary, [
    '--output-preset',
    'agent',
    '--color',
    'never',
    '--no-progress',
    '--diagnostic-format',
    'agent',
    'check',
    '--project',
    projectPath,
  ]);
  return { ...result, diagnostics: parseCompilerDiagnostics(result.output) };
}

function diagnosticMatches(
  actual: CompilerDiagnostic,
  expected: ExpectedDiagnostic,
): boolean {
  return (
    actual.code === expected.code &&
    (expected.messageContains === undefined ||
      actual.output.includes(expected.messageContains))
  );
}

export function expectationFailure(
  expectation: SnippetExpectation,
  result: CompilerCheckResult,
): string | null {
  if (result.exitCode !== 0 && result.diagnostics.length === 0) {
    return `compiler exited ${result.exitCode} without a typed diagnostic`;
  }
  if (expectation.status === 'success') {
    return result.diagnostics.length === 0
      ? null
      : `expected success but received ${result.diagnostics.length} error diagnostic(s)`;
  }
  if (result.diagnostics.length === 0) {
    return 'expected failure but received no error diagnostics';
  }
  const missing = expectation.diagnostics?.filter(
    (expected) =>
      !result.diagnostics.some((actual) => diagnosticMatches(actual, expected)),
  );
  if (missing && missing.length > 0) {
    return `missing expected diagnostics: ${missing
      .map(
        (diagnostic) =>
          `${diagnostic.code}${
            diagnostic.messageContains
              ? ` containing ${JSON.stringify(diagnostic.messageContains)}`
              : ''
          }`,
      )
      .join(', ')}`;
  }
  return null;
}

async function listSourceFiles(root: string): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(root, entry.name);
      if (entry.isDirectory()) return listSourceFiles(path);
      return /\.(mdx|tsx)$/.test(entry.name) ? [path] : [];
    }),
  );
  return files.flat();
}

async function findPageSources(
  appRoot: string,
  component: 'BamlProject' | 'BamlSnippet',
  id: string,
): Promise<string[]> {
  const roots = [resolve(appRoot, 'app'), resolve(appRoot, 'content')];
  const files = (await Promise.all(roots.map(listSourceFiles))).flat();
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const usagePattern = new RegExp(
    `<${component}\\s+[^>]*id=["']${escapedId}["']`,
  );
  const matched = await Promise.all(
    files.map(async (path) =>
      usagePattern.test(await readFile(path, 'utf8'))
        ? relative(appRoot, path).split(sep).join('/')
        : null,
    ),
  );
  return matched.filter((path): path is string => path !== null).sort();
}

function formatFailure(
  toolchainVersion: string,
  kind: 'project' | 'standalone',
  id: string,
  sourcePath: string,
  pageSources: readonly string[],
  expectation: SnippetExpectation,
  reason: string,
  result: CompilerCheckResult,
): Error {
  return new Error(
    [
      `BAML ${kind} snippet validation failed`,
      `id: ${id}`,
      `page: ${pageSources.join(', ') || '<unreferenced>'}`,
      `source: content/code/${sourcePath}`,
      `toolchain: ${toolchainVersion}`,
      `expectation: ${JSON.stringify(expectation)}`,
      `actual: ${reason}`,
      result.output.trim() || '<no compiler output>',
    ].join('\n'),
  );
}

async function validateStandalone(
  binary: string,
  toolchainVersion: string,
  appRoot: string,
  snippet: StandaloneSnippet,
): Promise<SnippetValidationResult> {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'baml-docs-snippet-'));
  const expectedPrefix = join(tmpdir(), 'baml-docs-snippet-');
  if (!temporaryRoot.startsWith(expectedPrefix)) {
    throw new Error(`Unexpected temporary path: ${temporaryRoot}`);
  }
  try {
    await copyFile(
      snippet.absolutePath,
      join(temporaryRoot, basename(snippet.absolutePath)),
    );
    const result = await checkProject(binary, temporaryRoot);
    const pageSources = await findPageSources(
      appRoot,
      'BamlSnippet',
      snippet.id,
    );
    const reason = expectationFailure(snippet.parsed.expectation, result);
    if (reason) {
      throw formatFailure(
        toolchainVersion,
        'standalone',
        snippet.id,
        snippet.sourcePath,
        pageSources,
        snippet.parsed.expectation,
        reason,
        result,
      );
    }
    return {
      id: snippet.id,
      kind: 'standalone',
      pageSources,
      sourcePath: snippet.sourcePath,
    };
  } finally {
    await rm(temporaryRoot, { force: true, recursive: true });
  }
}

async function validateProject(
  binary: string,
  toolchainVersion: string,
  appRoot: string,
  project: ProjectSnippet,
): Promise<SnippetValidationResult> {
  const result = await checkProject(binary, project.absolutePath);
  const pageSources = await findPageSources(appRoot, 'BamlProject', project.id);
  const reason = expectationFailure(project.expectation, result);
  if (reason) {
    throw formatFailure(
      toolchainVersion,
      'project',
      project.id,
      project.sourcePath,
      pageSources,
      project.expectation,
      reason,
      result,
    );
  }
  return {
    id: project.id,
    kind: 'project',
    pageSources,
    sourcePath: project.sourcePath,
  };
}

export async function validateSnippetCatalog(
  binary: string,
  appRoot: string,
): Promise<{ results: SnippetValidationResult[]; toolchainVersion: string }> {
  const versionResult = await runProcess(binary, ['--version']);
  if (versionResult.exitCode !== 0) {
    throw new Error(
      `Unable to read BAML toolchain version from ${binary}: ${versionResult.output}`,
    );
  }
  const toolchainVersion = versionResult.output.trim();
  const standaloneSnippets = await discoverStandaloneSnippets();
  const projects = await discoverProjectSnippets();
  const standaloneResults: SnippetValidationResult[] = [];
  for (const snippet of standaloneSnippets) {
    standaloneResults.push(
      await validateStandalone(binary, toolchainVersion, appRoot, snippet),
    );
  }
  const projectResults: SnippetValidationResult[] = [];
  for (const project of projects) {
    projectResults.push(
      await validateProject(binary, toolchainVersion, appRoot, project),
    );
  }
  return {
    results: [...standaloneResults, ...projectResults],
    toolchainVersion,
  };
}
