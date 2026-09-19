import assert from 'node:assert/strict';
import test from 'node:test';
import {
  bookFileIdentity,
  validateBookLayout,
} from '../lib/content/book-layout';
import { bookPageSchema } from '../lib/content/book-perspective-schema';
import {
  matchingSection,
  parseBookPerspective,
  perspectiveHref,
  resolveBookPerspective,
  validateBookVersions,
} from '../lib/content/book-perspectives';

test('an unsupported chapter falls back without changing the requested perspective', () => {
  const preference = parseBookPerspective('typescript');
  assert.ok(preference);
  assert.equal(preference, 'typescript');
  assert.equal(resolveBookPerspective(preference, ['default']), 'default');
  assert.equal(
    resolveBookPerspective(preference, ['default', 'typescript']),
    'typescript',
  );
  for (const invalid of ['rust', 'constructor', '', null, undefined]) {
    assert.equal(parseBookPerspective(invalid), undefined);
  }
});

test('section matching supports renamed, reordered, additional, and missing headings', () => {
  const general = { types: 'data-types', variables: 'declare-a-variable' };
  const typescript = { types: 'beyond-number', variables: 'familiar-let' };
  const headings = ['beyond-number', 'familiar-let', 'named-arguments'];
  assert.equal(
    matchingSection('declare-a-variable', general, typescript, headings),
    'familiar-let',
  );
  assert.equal(
    matchingSection('data-types', general, typescript, headings),
    'beyond-number',
  );
  assert.equal(
    matchingSection('named-arguments', {}, {}, headings),
    'named-arguments',
  );
  assert.equal(matchingSection('unmapped', {}, {}, headings), undefined);
  assert.equal(
    matchingSection(undefined, general, typescript, headings),
    undefined,
  );
});

test('shared links preserve the section and other query parameters', () => {
  assert.equal(
    perspectiveHref(
      '/baml/book/errors?example=one#handle-errors',
      'typescript',
    ),
    '/baml/book/errors?example=one&perspective=typescript#handle-errors',
  );
  assert.equal(
    perspectiveHref(
      '/baml/book/errors?perspective=typescript#handle-errors',
      'default',
    ),
    '/baml/book/errors?perspective=default#handle-errors',
  );
});

test('perspective frontmatter has no routing fields', () => {
  const metadata = {
    sectionKeys: { variables: 'familiar-let' },
    title: 'Concepts',
  };
  assert.equal(bookPageSchema.safeParse(metadata).success, true);
  for (const extra of [
    { perspective: 'typescript' },
    { chapter: '/baml/book/concepts' },
    { persective: 'typescript' },
    { sectionKeys: { variables: '#familiar-let' } },
  ])
    assert.equal(
      bookPageSchema.safeParse({ ...metadata, ...extra }).success,
      false,
    );
});

test('book paths encode shared chapters and perspective versions at one canonical URL', () => {
  const files = validateBookLayout([
    'index.mdx',
    'errors.mdx',
    'concepts/default.mdx',
    'concepts/typescript.mdx',
  ]);
  assert.deepEqual(
    files.map(({ chapter, perspective, kind }) => ({
      chapter,
      kind,
      perspective,
    })),
    [
      { chapter: '/baml/book', kind: 'shared', perspective: 'default' },
      { chapter: '/baml/book/errors', kind: 'shared', perspective: 'default' },
      {
        chapter: '/baml/book/concepts',
        kind: 'perspective',
        perspective: 'default',
      },
      {
        chapter: '/baml/book/concepts',
        kind: 'perspective',
        perspective: 'typescript',
      },
    ],
  );
  assert.doesNotThrow(() => validateBookLayout(['concepts/default.mdx']));
});

test('book layout rejects ambiguous chapters, missing defaults, and invalid filenames', () => {
  for (const paths of [
    ['concepts.mdx', 'concepts/default.mdx'],
    ['concepts.mdx', 'concepts/typescript.mdx'],
  ])
    assert.throws(
      () => validateBookLayout(paths),
      /both a file and a perspective directory/,
    );
  assert.throws(
    () => validateBookLayout(['concepts/typescript.mdx']),
    /requires default.mdx/,
  );
  assert.throws(
    () => validateBookLayout(['concepts/default.mdx', 'concepts/rust.mdx']),
    /Unknown book perspective/,
  );
  assert.throws(
    () => validateBookLayout(['errors.mdx', 'errors.mdx']),
    /Duplicate book file/,
  );
  for (const path of [
    'concepts/index.mdx',
    'concepts/typscript.mdx',
    'concepts/typescript/extra.mdx',
    '../errors.mdx',
    'Bad Name.mdx',
    'index/default.mdx',
  ]) {
    assert.throws(
      () => bookFileIdentity(path),
      /Invalid book path|Unknown book perspective/,
    );
  }
});

test('content validation rejects orphan versions, duplicates, and broken section mappings', () => {
  const base = {
    chapter: '/baml/book/concepts',
    headings: ['variables'],
    perspective: 'default' as const,
  };
  const variant = {
    chapter: base.chapter,
    headings: ['familiar-let'],
    perspective: 'typescript' as const,
  };
  assert.doesNotThrow(() => validateBookVersions([base, variant]));
  assert.throws(() => validateBookVersions([variant]), /no default chapter/);
  assert.throws(
    () => validateBookVersions([base, variant, variant]),
    /Duplicate/,
  );
  assert.throws(
    () =>
      validateBookVersions([
        { ...base, sectionKeys: { variables: 'missing' } },
      ]),
    /missing heading/,
  );
  assert.throws(
    () =>
      validateBookVersions([
        { ...base, sectionKeys: { first: 'variables', second: 'variables' } },
      ]),
    /multiple section keys/,
  );
});
