import * as fs from 'fs';
import * as path from 'path';

export interface CanonicalPath {
  /** Resolved path used for filesystem access. */
  fsPath: string;
  /** Filesystem-aware identity used as a map key. */
  key: string;
}

export interface BamlProjectRoots {
  canonicalInput: CanonicalPath;
  ancestors: CanonicalPath[];
  semanticRoot: CanonicalPath | undefined;
  ownershipRoot: CanonicalPath | undefined;
}

export interface RoutableOwnershipPattern {
  basePath: string;
  pattern: string;
}

export type InputPathKind = 'file' | 'directory' | 'auto';

function toggleAsciiCase(value: string): string | undefined {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 65 && code <= 90) {
      return `${value.slice(0, index)}${value[index]!.toLowerCase()}${value.slice(index + 1)}`;
    }
    if (code >= 97 && code <= 122) {
      return `${value.slice(0, index)}${value[index]!.toUpperCase()}${value.slice(index + 1)}`;
    }
  }
  return undefined;
}

function sameFile(left: fs.Stats, right: fs.Stats): boolean {
  return left.dev === right.dev && left.ino === right.ino;
}

/**
 * Determine case behavior without writing a probe file into the user's project.
 * On case-insensitive filesystems, changing the case of an existing component
 * resolves to the same inode.
 */
function isCaseSensitive(existingPath: string): boolean {
  let current = existingPath;
  while (true) {
    const parent = path.dirname(current);
    if (parent === current) {
      return process.platform !== 'win32';
    }

    const alternateName = toggleAsciiCase(path.basename(current));
    if (alternateName !== undefined) {
      const alternate = path.join(parent, alternateName);
      try {
        return !sameFile(fs.statSync(current), fs.statSync(alternate));
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
          return true;
        }
      }
    }

    current = parent;
  }
}

export function pathIdentity(resolvedPath: string, caseSensitive: boolean): string {
  const normalized = path.normalize(resolvedPath);
  return caseSensitive ? normalized : normalized.toLowerCase();
}

function nearestExistingAncestor(absolutePath: string): {
  existingPath: string | undefined;
  missingComponents: string[];
} {
  let candidate = absolutePath;
  const missingComponents: string[] = [];

  while (!fs.existsSync(candidate)) {
    const parent = path.dirname(candidate);
    if (parent === candidate) {
      return { existingPath: undefined, missingComponents };
    }
    missingComponents.unshift(path.basename(candidate));
    candidate = parent;
  }

  return { existingPath: candidate, missingComponents };
}

/**
 * Produce one path identity for routing. Callers should pass `Uri.fsPath`, which
 * is already URI-decoded by VS Code. Symlinks are resolved through the nearest
 * existing ancestor so newly-created documents receive the same identity as
 * existing documents beneath the same alias.
 */
export function canonicalPathIdentity(inputPath: string): CanonicalPath {
  const absolutePath = path.resolve(inputPath);
  const { existingPath, missingComponents } = nearestExistingAncestor(absolutePath);

  if (existingPath === undefined) {
    const normalized = path.normalize(absolutePath);
    return {
      fsPath: normalized,
      key: pathIdentity(normalized, process.platform !== 'win32'),
    };
  }

  let realExistingPath: string;
  try {
    realExistingPath = fs.realpathSync.native(existingPath);
  } catch {
    realExistingPath = fs.realpathSync(existingPath);
  }

  const resolvedPath = path.normalize(path.join(realExistingPath, ...missingComponents));
  return {
    fsPath: resolvedPath,
    key: pathIdentity(resolvedPath, isCaseSensitive(realExistingPath)),
  };
}

function startDirectory(input: CanonicalPath, kind: InputPathKind): CanonicalPath {
  if (kind === 'directory') {
    return input;
  }
  if (kind === 'file') {
    return canonicalPathIdentity(path.dirname(input.fsPath));
  }

  try {
    if (fs.statSync(input.fsPath).isDirectory()) {
      return input;
    }
  } catch {
    // Missing command paths are file-like. Their nearest existing ancestor was
    // still canonicalized above.
  }
  return canonicalPathIdentity(path.dirname(input.fsPath));
}

export function canonicalAncestorDirectories(directoryPath: string): CanonicalPath[] {
  const result: CanonicalPath[] = [];
  let current = canonicalPathIdentity(directoryPath);

  while (true) {
    result.push(current);
    const parentPath = path.dirname(current.fsPath);
    if (parentPath === current.fsPath) {
      break;
    }
    current = canonicalPathIdentity(parentPath);
  }

  return result;
}

function isFile(filePath: string): boolean {
  try {
    return fs.statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function isDirectory(directoryPath: string): boolean {
  try {
    return fs.statSync(directoryPath).isDirectory();
  } catch {
    return false;
  }
}

export function hasBamlProjectMarker(directoryPath: string): boolean {
  return isFile(path.join(directoryPath, 'baml.toml'))
    || isDirectory(path.join(directoryPath, 'baml_src'));
}

export function resolveBamlProjectRoots(
  inputPath: string,
  kind: InputPathKind = 'auto',
): BamlProjectRoots {
  const canonicalInput = canonicalPathIdentity(inputPath);
  const directory = startDirectory(canonicalInput, kind);
  const ancestors = canonicalAncestorDirectories(directory.fsPath);
  let semanticRoot: CanonicalPath | undefined;
  let ownershipRoot: CanonicalPath | undefined;

  for (const ancestor of ancestors) {
    if (!hasBamlProjectMarker(ancestor.fsPath)) {
      continue;
    }
    semanticRoot ??= ancestor;
    ownershipRoot = ancestor;
  }

  return {
    canonicalInput,
    ancestors,
    semanticRoot,
    ownershipRoot,
  };
}

export function resolveSemanticProjectRoot(
  inputPath: string,
  kind: InputPathKind = 'auto',
): CanonicalPath | undefined {
  return resolveBamlProjectRoots(inputPath, kind).semanticRoot;
}

export function resolveOwnershipRoot(
  inputPath: string,
  kind: InputPathKind = 'auto',
): CanonicalPath | undefined {
  return resolveBamlProjectRoots(inputPath, kind).ownershipRoot;
}

/**
 * Recover the path spelling of an ownership root from the path spelling of a
 * document. Ownership identity stays canonical, but VS Code's
 * `RelativePattern` matches URI paths and therefore also needs the symlink
 * alias through which the document was opened.
 */
export function routableOwnershipRoot(
  inputPath: string,
  roots: BamlProjectRoots,
): string | undefined {
  return routableOwnershipMatch(inputPath, roots)?.basePath;
}

function routableOwnershipMatch(
  inputPath: string,
  roots: BamlProjectRoots,
): { basePath: string; matchedSuffixComponents: number } | undefined {
  const owner = roots.ownershipRoot;
  if (!owner) return undefined;

  const relative = path.relative(owner.fsPath, roots.canonicalInput.fsPath);
  if (
    relative === '' ||
    path.isAbsolute(relative) ||
    relative === '..' ||
    relative.startsWith(`..${path.sep}`)
  ) {
    return { basePath: owner.fsPath, matchedSuffixComponents: 1 };
  }

  const relativeComponents = relative.split(path.sep).filter(Boolean);
  let candidate = path.resolve(inputPath);
  let matchedSuffixComponents = 0;
  for (
    let index = relativeComponents.length - 1;
    index >= 0 &&
    path.basename(candidate) === relativeComponents[index];
    index -= 1
  ) {
    candidate = path.dirname(candidate);
    matchedSuffixComponents += 1;
  }

  // A symlink may enter below the canonical owner (for example `alias-src`
  // points at `project/src`). In that case only the shared suffix is safe as a
  // selector base; ascending the full canonical relative path would broaden
  // the selector above the alias and could overlap another owner's client.
  return {
    basePath:
      matchedSuffixComponents > 0 ? candidate : path.dirname(inputPath),
    matchedSuffixComponents,
  };
}

function exactGlobSegment(value: string): string {
  return value.replace(/[\[\]*?{}]/g, (character) => `[${character}]`);
}

/**
 * Build the narrowest RelativePattern needed for the URI spelling that was
 * opened. Directory aliases can safely own their subtree. A direct file
 * symlink (or an alias with no canonical suffix match) must use an exact file
 * pattern; broadening to its parent would overlap sibling project clients.
 */
export function routableOwnershipPattern(
  inputPath: string,
  roots: BamlProjectRoots,
): RoutableOwnershipPattern | undefined {
  const match = routableOwnershipMatch(inputPath, roots);
  if (!match) return undefined;

  let directFileSymlink = false;
  try {
    directFileSymlink =
      fs.lstatSync(path.resolve(inputPath)).isSymbolicLink() &&
      fs.statSync(path.resolve(inputPath)).isFile();
  } catch {
    // Missing documents are covered by the suffix-based fallback below.
  }

  if (directFileSymlink || match.matchedSuffixComponents === 0) {
    return {
      basePath: path.dirname(inputPath),
      pattern: exactGlobSegment(path.basename(inputPath)),
    };
  }

  return { basePath: match.basePath, pattern: '**/*.baml' };
}
