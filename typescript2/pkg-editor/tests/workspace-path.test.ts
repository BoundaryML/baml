import { URI } from '@codingame/monaco-vscode-api/vscode/vs/base/common/uri';
import { describe, expect, it } from 'vitest';
import { createWorkspacePathModel } from '../src/workspace-path';

describe('workspace path model', () => {
  it('uses one URI identity for a native Windows root and its files', () => {
    const paths = createWorkspacePathModel(
      URI,
      'D:\\Repo with space\\100%\\baml_src\\',
    );

    expect(paths.rootUri.path).toBe('/D:/Repo with space/100%/baml_src');
    expect(paths.rootUri.toString()).toBe(
      'file:///d%3A/Repo%20with%20space/100%25/baml_src',
    );
    expect(paths.configUri.path).toBe(
      '/D:/Repo with space/100%/baml_src.code-workspace',
    );

    const file = paths.fileUri('nested\\main.baml');
    expect(file.path).toBe(
      '/D:/Repo with space/100%/baml_src/nested/main.baml',
    );
    expect(paths.isAllowedUri(file)).toBe(true);
    expect(paths.relativeFilename(file)).toBe('nested/main.baml');
  });

  it('treats slash and backslash spellings of a Windows root identically', () => {
    const native = createWorkspacePathModel(URI, 'D:\\repo\\baml_src');
    const slash = createWorkspacePathModel(URI, 'D:/repo/baml_src');
    const vfs = createWorkspacePathModel(URI, '/D:/repo/baml_src');

    expect(native.rootUri.toString()).toBe(slash.rootUri.toString());
    expect(slash.rootUri.toString()).toBe(vfs.rootUri.toString());
    expect(native.fileUri('main.baml').toString()).toBe(
      vfs.fileUri('main.baml').toString(),
    );
  });

  it('preserves UNC authority while deriving descendants and ancestors', () => {
    const paths = createWorkspacePathModel(
      URI,
      '\\\\server\\share\\repo\\baml_src',
    );

    expect(paths.rootUri.authority).toBe('server');
    expect(paths.rootUri.path).toBe('/share/repo/baml_src');
    const file = paths.fileUri('nested/main.baml');
    expect(file.toString()).toBe(
      'file://server/share/repo/baml_src/nested/main.baml',
    );
    expect(paths.relativeFilename(URI.parse(file.toString()))).toBe(
      'nested/main.baml',
    );
    expect(paths.rootAncestorUris().map((uri) => uri.toString())).toEqual([
      'file://server/share',
      'file://server/share/repo',
      'file://server/share/repo/baml_src',
    ]);
  });

  it('uses component boundaries and URI authority for sandbox containment', () => {
    const paths = createWorkspacePathModel(URI, 'D:\\repo\\baml_src');

    expect(paths.isAllowedUri(paths.rootUri)).toBe(true);
    expect(paths.isAllowedUri(paths.configUri)).toBe(true);
    expect(
      paths.isAllowedUri(URI.file('D:\\repo\\baml_src-other\\main.baml')),
    ).toBe(false);
    expect(
      paths.isAllowedUri(URI.parse('untitled:/D:/repo/baml_src/main.baml')),
    ).toBe(false);
    expect(
      paths.relativeFilename(
        URI.parse('file://other/D:/repo/baml_src/main.baml'),
      ),
    ).toBeNull();
  });

  it('recognizes decorated aliases of the workspace root', () => {
    const paths = createWorkspacePathModel(URI, '/workspace');

    for (const decoratedRoot of [
      paths.rootUri.with({ query: 'query' }),
      paths.rootUri.with({ fragment: 'fragment' }),
      paths.rootUri.with({ fragment: 'fragment', query: 'query' }),
    ]) {
      expect(paths.isAllowedUri(decoratedRoot)).toBe(true);
      expect(paths.isRootUri(decoratedRoot)).toBe(true);
      for (const operation of ['delete', 'rename', 'rename to']) {
        expect(() => paths.assertNotRootUri(decoratedRoot, operation)).toThrow(
          `cannot ${operation} the workspace root directory`,
        );
      }
    }

    const decoratedChild = paths.fileUri('main.baml').with({ query: 'query' });
    expect(paths.isAllowedUri(decoratedChild)).toBe(true);
    expect(paths.isRootUri(decoratedChild)).toBe(false);
    expect(() =>
      paths.assertNotRootUri(decoratedChild, 'delete'),
    ).not.toThrow();
  });

  it('normalizes portable relative keys and rejects ambiguous or escaping keys', () => {
    const paths = createWorkspacePathModel(URI, '/workspace');

    expect(paths.normalizeFilename('nested\\main.baml')).toBe(
      'nested/main.baml',
    );
    expect(
      paths.parentDirectoryUris('a\\b\\main.baml').map((uri) => uri.path),
    ).toEqual(['/workspace/a', '/workspace/a/b']);

    for (const filename of [
      '',
      '/absolute.baml',
      'C:\\other\\main.baml',
      '../outside.baml',
      'nested/../outside.baml',
      './main.baml',
      'nested//main.baml',
    ]) {
      expect(() => paths.fileUri(filename), filename).toThrow(
        /normalized relative path/,
      );
    }
  });

  it('does not silently fold URI path case', () => {
    const paths = createWorkspacePathModel(URI, 'D:\\Repo\\baml_src');
    const differentlyCased = URI.file('D:\\repo\\baml_src\\main.baml');

    expect(paths.relativeFilename(differentlyCased)).toBeNull();
    expect(paths.isAllowedUri(differentlyCased)).toBe(false);
  });
});
