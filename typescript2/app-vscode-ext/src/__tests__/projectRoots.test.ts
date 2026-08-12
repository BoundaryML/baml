import {
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'fs';
import { tmpdir } from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  canonicalAncestorDirectories,
  canonicalPathIdentity,
  pathIdentity,
  resolveBamlProjectRoots,
  resolveOwnershipRoot,
  routableOwnershipPattern,
  routableOwnershipRoot,
  resolveSemanticProjectRoot,
} from '../projectRoots';

describe('BAML project root resolution', () => {
  let sandbox: string;

  beforeEach(() => {
    sandbox = mkdtempSync(path.join(tmpdir(), 'baml-project-roots-'));
  });

  afterEach(() => {
    rmSync(sandbox, { recursive: true, force: true });
  });

  it('uses the nearest marker for semantics and the outermost marker for ownership', () => {
    const outer = path.join(sandbox, 'outer');
    const nested = path.join(outer, 'packages', 'nested');
    const outerModel = path.join(outer, 'outer.baml');
    const model = path.join(nested, 'baml_src', 'models', 'main.baml');
    mkdirSync(path.dirname(model), { recursive: true });
    writeFileSync(path.join(outer, 'baml.toml'), '');
    writeFileSync(outerModel, 'class Outer {}');
    writeFileSync(model, 'class Main {}');

    const roots = resolveBamlProjectRoots(model, 'file');

    expect(roots.semanticRoot?.fsPath).toBe(realpathSync(nested));
    expect(roots.ownershipRoot?.fsPath).toBe(realpathSync(outer));
    expect(resolveSemanticProjectRoot(model, 'file')?.key).toBe(roots.semanticRoot?.key);
    expect(resolveOwnershipRoot(model, 'file')?.key).toBe(roots.ownershipRoot?.key);
    expect(resolveOwnershipRoot(outerModel, 'file')?.key).toBe(roots.ownershipRoot?.key);
    expect(resolveOwnershipRoot(nested, 'directory')?.key).toBe(roots.ownershipRoot?.key);
  });

  it('keeps sibling top-level projects in separate ownership domains', () => {
    const leftFile = path.join(sandbox, 'left', 'baml_src', 'left.baml');
    const rightFile = path.join(sandbox, 'right', 'baml_src', 'right.baml');
    mkdirSync(path.dirname(leftFile), { recursive: true });
    mkdirSync(path.dirname(rightFile), { recursive: true });
    writeFileSync(leftFile, 'class Left {}');
    writeFileSync(rightFile, 'class Right {}');

    const leftOwner = resolveOwnershipRoot(leftFile, 'file');
    const rightOwner = resolveOwnershipRoot(rightFile, 'file');

    expect(leftOwner?.fsPath).toBe(realpathSync(path.join(sandbox, 'left')));
    expect(rightOwner?.fsPath).toBe(realpathSync(path.join(sandbox, 'right')));
    expect(leftOwner?.key).not.toBe(rightOwner?.key);
  });

  it('does not invent an owner for an unmarked standalone file', () => {
    const model = path.join(sandbox, 'standalone', 'main.baml');
    mkdirSync(path.dirname(model), { recursive: true });
    writeFileSync(model, 'class Main {}');

    const roots = resolveBamlProjectRoots(model, 'file');

    expect(roots.semanticRoot).toBeUndefined();
    expect(roots.ownershipRoot).toBeUndefined();
  });

  it('requires baml_src to be a directory', () => {
    const project = path.join(sandbox, 'project');
    const model = path.join(project, 'main.baml');
    mkdirSync(project, { recursive: true });
    writeFileSync(path.join(project, 'baml_src'), 'not a directory');
    writeFileSync(model, 'class Main {}');

    expect(resolveOwnershipRoot(model, 'file')).toBeUndefined();
  });

  it('observes marker additions without a stale resolver cache', () => {
    const project = path.join(sandbox, 'project');
    const model = path.join(project, 'main.baml');
    mkdirSync(project, { recursive: true });
    writeFileSync(model, 'class Main {}');
    expect(resolveOwnershipRoot(model, 'file')).toBeUndefined();

    writeFileSync(path.join(project, 'baml.toml'), '');

    expect(resolveOwnershipRoot(model, 'file')?.fsPath).toBe(realpathSync(project));

    rmSync(path.join(project, 'baml.toml'));

    expect(resolveOwnershipRoot(model, 'file')).toBeUndefined();
  });

  it.skipIf(process.platform === 'win32')(
    'resolves symlinks through the nearest existing ancestor and preserves spaces',
    () => {
      const project = path.join(sandbox, 'project with spaces');
      const source = path.join(project, 'src');
      const model = path.join(source, 'main.baml');
      const alias = path.join(sandbox, 'alias');
      const sourceAlias = path.join(sandbox, 'source-alias');
      const fileAlias = path.join(sandbox, 'direct-main.baml');
      mkdirSync(source, { recursive: true });
      writeFileSync(path.join(project, 'baml.toml'), '');
      writeFileSync(model, 'class Main {}');
      symlinkSync(project, alias, 'dir');
      symlinkSync(source, sourceAlias, 'dir');
      symlinkSync(model, fileAlias, 'file');

      const direct = canonicalPathIdentity(model);
      const aliased = canonicalPathIdentity(path.join(alias, 'src', '..', 'src', 'main.baml'));
      const missingOwner = resolveOwnershipRoot(
        path.join(alias, 'generated', 'not-created-yet.baml'),
        'file',
      );

      expect(aliased.key).toBe(direct.key);
      expect(aliased.fsPath).toBe(direct.fsPath);
      expect(missingOwner?.key).toBe(canonicalPathIdentity(project).key);

      const aliasRoots = resolveBamlProjectRoots(
        path.join(alias, 'src', 'main.baml'),
        'file',
      );
      expect(
        routableOwnershipRoot(path.join(alias, 'src', 'main.baml'), aliasRoots),
      ).toBe(alias);

      const sourceAliasRoots = resolveBamlProjectRoots(
        path.join(sourceAlias, 'main.baml'),
        'file',
      );
      expect(
        routableOwnershipRoot(
          path.join(sourceAlias, 'main.baml'),
          sourceAliasRoots,
        ),
      ).toBe(sourceAlias);

      const fileAliasRoots = resolveBamlProjectRoots(fileAlias, 'file');
      expect(routableOwnershipPattern(fileAlias, fileAliasRoots)).toEqual({
        basePath: sandbox,
        pattern: 'direct-main.baml',
      });
      expect(
        routableOwnershipPattern(
          path.join(sourceAlias, 'main.baml'),
          sourceAliasRoots,
        ),
      ).toEqual({ basePath: sourceAlias, pattern: '**/*.baml' });
    },
  );

  it('builds a component-by-component ancestor chain through the volume root', () => {
    const nested = path.join(sandbox, 'one', 'two');
    mkdirSync(nested, { recursive: true });

    const ancestors = canonicalAncestorDirectories(nested);

    expect(ancestors[0]?.fsPath).toBe(realpathSync(nested));
    expect(ancestors.at(-1)?.fsPath).toBe(path.parse(nested).root);
    expect(new Set(ancestors.map((ancestor) => ancestor.key)).size).toBe(ancestors.length);
  });

  it('folds identity case only for case-insensitive filesystems', () => {
    const mixedCase = path.join(path.parse(sandbox).root, 'Users', 'Example', 'Project');

    expect(pathIdentity(mixedCase, true)).toBe(path.normalize(mixedCase));
    expect(pathIdentity(mixedCase, false)).toBe(path.normalize(mixedCase).toLowerCase());
  });
});
