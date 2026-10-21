interface UriLike<U> {
  readonly scheme: string;
  readonly authority: string;
  readonly path: string;
  with(change: { authority?: string; path?: string }): U;
}

interface UriApi<U> {
  file(path: string): U;
  joinPath(base: U, ...pathSegments: string[]): U;
}

function normalizeRootPath(path: string): string {
  if (path === '/' || /^\/[A-Za-z]:\/$/.test(path)) return path;
  return path.replace(/\/+$/, '') || '/';
}

function workspaceRootUri<U extends UriLike<U>>(
  uri: UriApi<U>,
  workspaceRoot: string,
): U {
  const normalized = (workspaceRoot || '/workspace').replace(/\\/g, '/');
  const unc = normalized.match(/^\/\/([^/]+)(\/.*)?$/);
  if (unc) {
    return uri.file(unc[2] || '/').with({ authority: unc[1] });
  }

  const drive = normalized.match(/^\/?([A-Za-z]:)(\/.*)?$/);
  if (drive) {
    return uri.file(`/${drive[1]}${drive[2] || '/'}`);
  }

  return uri.file(normalized);
}

function splitRelativeFilename(filename: string): string[] {
  const normalized = filename.replace(/\\/g, '/');
  const parts = normalized.split('/');

  if (
    normalized.length === 0 ||
    normalized.startsWith('/') ||
    /^[A-Za-z]:(?:\/|$)/.test(normalized) ||
    parts.some((part) => part === '' || part === '.' || part === '..')
  ) {
    throw new Error(
      `Workspace filename must be a normalized relative path: ${filename}`,
    );
  }

  return parts;
}

/**
 * Owns the boundary between a host-native workspace root and Monaco's file
 * URIs. File-map keys remain portable, workspace-relative slash paths; all
 * absolute identity and containment checks use the URI representation.
 */
export function createWorkspacePathModel<U extends UriLike<U>>(
  uri: UriApi<U>,
  workspaceRoot: string,
) {
  const parsedRoot = workspaceRootUri(uri, workspaceRoot);
  const rootPath = normalizeRootPath(parsedRoot.path);
  const rootUri =
    parsedRoot.path === rootPath
      ? parsedRoot
      : parsedRoot.with({ path: rootPath });
  const configUri = rootUri.with({ path: `${rootPath}.code-workspace` });
  const rootPrefix = rootPath.endsWith('/') ? rootPath : `${rootPath}/`;
  const rootScheme = rootUri.scheme.toLowerCase();
  const rootAuthority = rootUri.authority.toLowerCase();

  const hasWorkspaceIdentity = (
    candidate: Pick<U, 'scheme' | 'authority'>,
  ): boolean =>
    candidate.scheme.toLowerCase() === rootScheme &&
    candidate.authority.toLowerCase() === rootAuthority;

  const isRootUri = (candidate: U): boolean =>
    hasWorkspaceIdentity(candidate) && candidate.path === rootPath;

  const assertNotRootUri = (candidate: U, operation: string): void => {
    if (isRootUri(candidate)) {
      throw new Error(
        `Sandbox violation: cannot ${operation} the workspace root directory`,
      );
    }
  };

  const isWorkspaceUri = (candidate: U): boolean =>
    isRootUri(candidate) ||
    (hasWorkspaceIdentity(candidate) && candidate.path.startsWith(rootPrefix));

  const isAllowedUri = (candidate: U): boolean =>
    isWorkspaceUri(candidate) ||
    (hasWorkspaceIdentity(candidate) && candidate.path === configUri.path);

  const normalizeFilename = (filename: string): string =>
    splitRelativeFilename(filename).join('/');

  const fileUri = (filename: string): U => {
    const normalizedFilename = normalizeFilename(filename);
    const result = uri.joinPath(rootUri, ...normalizedFilename.split('/'));
    if (!isWorkspaceUri(result) || result.path === rootPath) {
      throw new Error(`Workspace filename escapes the workspace: ${filename}`);
    }
    return result;
  };

  const relativeFilename = (candidate: U): string | null => {
    if (
      !hasWorkspaceIdentity(candidate) ||
      !candidate.path.startsWith(rootPrefix)
    ) {
      return null;
    }
    return candidate.path.slice(rootPrefix.length);
  };

  const rootAncestorUris = (): U[] => {
    if (rootPath === '/') return [];

    const ancestorPaths: string[] = [];
    for (let i = 1; i < rootPath.length; i++) {
      if (rootPath[i] === '/') ancestorPaths.push(rootPath.slice(0, i));
    }
    ancestorPaths.push(rootPath);

    return [...new Set(ancestorPaths)]
      .filter((path) => path !== '')
      .map((path) => rootUri.with({ path }));
  };

  const parentDirectoryUris = (filename: string): U[] => {
    const parts = normalizeFilename(filename).split('/');
    const directories: U[] = [];
    for (let i = 1; i < parts.length; i++) {
      directories.push(uri.joinPath(rootUri, ...parts.slice(0, i)));
    }
    return directories;
  };

  return {
    assertNotRootUri,
    configUri,
    fileUri,
    isAllowedUri,
    isRootUri,
    normalizeFilename,
    parentDirectoryUris,
    relativeFilename,
    rootAncestorUris,
    rootPath,
    rootUri,
  };
}
