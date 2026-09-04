import { readdir, readFile, stat } from 'node:fs/promises';
import { extname, relative, resolve, sep } from 'node:path';

import { type ParsedBamlSource, parseBamlSource } from './parser';
import {
  type SnippetExpectation,
  successfulSnippetExpectation,
} from './schema';

export const snippetContentRoot = resolve(process.cwd(), 'content/code');

export interface StandaloneSnippet {
  absolutePath: string;
  id: string;
  parsed: ParsedBamlSource;
  sourcePath: string;
}

export interface ProjectSnippetFile {
  absolutePath: string;
  displaySource: string;
  projectPath: string;
  sourcePath: string;
}

export interface ProjectSnippet {
  absolutePath: string;
  expectation: SnippetExpectation;
  files: ProjectSnippetFile[];
  id: string;
  sourcePath: string;
}

function posixPath(path: string): string {
  return path.split(sep).join('/');
}

function resolveContained(root: string, requestedPath: string): string {
  if (
    !requestedPath ||
    requestedPath.startsWith('/') ||
    requestedPath.includes('\\')
  ) {
    throw new Error(`Invalid snippet path: ${requestedPath}`);
  }
  const absolutePath = resolve(root, requestedPath);
  const relativePath = relative(root, absolutePath);
  if (
    relativePath === '..' ||
    relativePath.startsWith(`..${sep}`) ||
    relativePath === ''
  ) {
    throw new Error(
      `Snippet path escapes its canonical root: ${requestedPath}`,
    );
  }
  return absolutePath;
}

async function listFiles(root: string): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(root, entry.name);
      return entry.isDirectory() ? listFiles(path) : [path];
    }),
  );
  return nested.flat().sort();
}

async function requireFile(path: string, label: string): Promise<void> {
  const details = await stat(path).catch(() => null);
  if (!details?.isFile()) {
    throw new Error(`${label} does not exist: ${path}`);
  }
}

async function requireDirectory(path: string, label: string): Promise<void> {
  const details = await stat(path).catch(() => null);
  if (!details?.isDirectory()) {
    throw new Error(`${label} does not exist: ${path}`);
  }
}

export async function loadStandaloneSnippet(
  id: string,
): Promise<StandaloneSnippet> {
  if (extname(id)) {
    throw new Error(`Standalone snippet IDs must omit the extension: ${id}`);
  }
  const standaloneRoot = resolve(snippetContentRoot, 'standalone');
  const absolutePath = resolveContained(standaloneRoot, `${id}.baml`);
  await requireFile(absolutePath, 'Standalone snippet');
  const sourcePath = posixPath(relative(snippetContentRoot, absolutePath));
  const rawSource = await readFile(absolutePath, 'utf8');
  return {
    absolutePath,
    id,
    parsed: parseBamlSource(rawSource, sourcePath),
    sourcePath,
  };
}

export async function discoverStandaloneSnippets(): Promise<
  StandaloneSnippet[]
> {
  const standaloneRoot = resolve(snippetContentRoot, 'standalone');
  const files = (await listFiles(standaloneRoot)).filter(
    (path) => extname(path) === '.baml',
  );
  return Promise.all(
    files.map((path) =>
      loadStandaloneSnippet(
        posixPath(relative(standaloneRoot, path)).replace(/\.baml$/, ''),
      ),
    ),
  );
}

export async function loadProjectSnippet(id: string): Promise<ProjectSnippet> {
  const projectsRoot = resolve(snippetContentRoot, 'projects');
  const absolutePath = resolveContained(projectsRoot, id);
  await requireDirectory(absolutePath, 'Snippet project');

  const manifestPath = resolveContained(absolutePath, 'baml.toml');
  const sourceRoot = resolveContained(absolutePath, 'baml_src');
  await requireFile(manifestPath, 'Snippet project manifest');
  await requireDirectory(sourceRoot, 'Snippet project source directory');

  const bamlFiles = (await listFiles(sourceRoot)).filter(
    (path) => extname(path) === '.baml',
  );
  if (bamlFiles.length === 0) {
    throw new Error(`Snippet project has no BAML source files: ${id}`);
  }

  const parsedFiles = await Promise.all(
    bamlFiles.map(async (path) => {
      const projectPath = posixPath(relative(absolutePath, path));
      const sourcePath = posixPath(relative(snippetContentRoot, path));
      const parsed = parseBamlSource(await readFile(path, 'utf8'), sourcePath);
      return { parsed, path, projectPath, sourcePath };
    }),
  );
  const metadataFiles = parsedFiles.filter(({ parsed }) => parsed.hasMetadata);
  if (metadataFiles.length > 1) {
    throw new Error(`Snippet project ${id} has metadata in more than one file`);
  }

  const files: ProjectSnippetFile[] = [
    {
      absolutePath: manifestPath,
      displaySource: (await readFile(manifestPath, 'utf8')).trim(),
      projectPath: 'baml.toml',
      sourcePath: posixPath(relative(snippetContentRoot, manifestPath)),
    },
    ...parsedFiles.map(({ path, parsed, projectPath, sourcePath }) => ({
      absolutePath: path,
      displaySource: parsed.source,
      projectPath,
      sourcePath,
    })),
  ];

  return {
    absolutePath,
    expectation:
      metadataFiles[0]?.parsed.expectation ?? successfulSnippetExpectation,
    files,
    id,
    sourcePath: posixPath(relative(snippetContentRoot, absolutePath)),
  };
}

export async function discoverProjectSnippets(): Promise<ProjectSnippet[]> {
  const projectsRoot = resolve(snippetContentRoot, 'projects');
  const entries = await readdir(projectsRoot, { withFileTypes: true });
  return Promise.all(
    entries
      .filter((entry) => entry.isDirectory())
      .map((entry) => loadProjectSnippet(entry.name)),
  );
}
