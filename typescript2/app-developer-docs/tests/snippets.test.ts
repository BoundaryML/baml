import assert from 'node:assert/strict';
import test from 'node:test';

import {
  type CompilerCheckResult,
  expectationFailure,
  parseCompilerDiagnostics,
} from '../lib/snippets/checker';
import {
  loadProjectSnippet,
  loadStandaloneSnippet,
} from '../lib/snippets/discovery';
import { highlightCode } from '../lib/snippets/highlighter';
import { parseBamlSource } from '../lib/snippets/parser';

test('snippet metadata and named regions are parsed and removed from display source', () => {
  const parsed = parseBamlSource(
    `// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
//       messageContains: expected string
// docs:endmeta
// docs:start example
function Broken(value: int) -> string {
  value
}
// docs:end example`,
    'fixture.baml',
  );

  assert.deepEqual(parsed.expectation, {
    diagnostics: [{ code: 'E0001', messageContains: 'expected string' }],
    status: 'failure',
  });
  assert.equal(parsed.hasMetadata, true);
  assert.match(parsed.regions.get('example') ?? '', /function Broken/);
  assert.doesNotMatch(parsed.source, /docs:/);
});

test('a source without markers exposes the whole file as the example region', () => {
  const parsed = parseBamlSource(
    'function Identity(value: int) -> int {\n  value\n}',
    'identity.baml',
  );
  assert.equal(parsed.regions.get('example'), parsed.source);
  assert.deepEqual(parsed.expectation, { status: 'success' });
});

test('snippet directives and metadata fail closed', () => {
  assert.throws(
    () =>
      parseBamlSource(
        '// docs:start first\n// docs:start second\n// docs:end second',
        'nested.baml',
      ),
    /nested inside/,
  );
  assert.throws(
    () =>
      parseBamlSource(
        '// docs:meta\n// expect:\n//   status: success\n//   surprise: true\n// docs:endmeta',
        'unknown.baml',
      ),
    /invalid docs metadata/,
  );
  assert.throws(
    () =>
      parseBamlSource(
        '// docs:meta\n// expect:\n//   status: success\n//   diagnostics: []\n// docs:endmeta',
        'contradictory.baml',
      ),
    /invalid docs metadata/,
  );
});

test('compiler diagnostics are parsed even when the compiler exits successfully', () => {
  const output = [
    'example.baml:1:1-1:2 error[E0001]: mismatched types',
    '  primary: expected `string`, found `int`',
    'example.baml:2:1-2:2 error[E0003]: unresolved name: `missing`',
  ].join('\n');
  const diagnostics = parseCompilerDiagnostics(output);
  assert.deepEqual(
    diagnostics.map(({ code, message }) => ({ code, message })),
    [
      { code: 'E0001', message: 'mismatched types' },
      { code: 'E0003', message: 'unresolved name: `missing`' },
    ],
  );

  const result: CompilerCheckResult = {
    diagnostics,
    exitCode: 0,
    output,
  };
  assert.equal(
    expectationFailure({ status: 'success' }, result),
    'expected success but received 2 error diagnostic(s)',
  );
  assert.equal(
    expectationFailure(
      {
        diagnostics: [{ code: 'E0001', messageContains: 'expected `string`' }],
        status: 'failure',
      },
      result,
    ),
    null,
  );
  assert.equal(
    expectationFailure(
      { diagnostics: [{ code: 'E0001' }], status: 'failure' },
      { ...result, exitCode: 2 },
    ),
    null,
  );
});

test('canonical standalone and project IDs resolve to checked source trees', async () => {
  const standalone = await loadStandaloneSnippet('functions/return-number');
  assert.equal(
    standalone.sourcePath,
    'standalone/functions/return-number.baml',
  );
  await assert.rejects(loadStandaloneSnippet('../outside'), /escapes/);

  const project = await loadProjectSnippet('cross-file-types');
  assert.deepEqual(
    project.files.map(({ projectPath }) => projectPath),
    ['baml.toml', 'baml_src/functions.baml', 'baml_src/types.baml'],
  );
  assert.deepEqual(project.expectation, { status: 'success' });
});

test('the canonical BAML grammar produces highlighted light and dark tokens', async () => {
  const highlighted = await highlightCode(
    'function Identity(value: int) -> int { value }',
    'baml',
  );
  for (const tokens of [highlighted.light, highlighted.dark]) {
    assert.equal(
      tokens
        .flat()
        .map(({ content }) => content)
        .join(''),
      'function Identity(value: int) -> int { value }',
    );
    assert.ok(tokens.flat().some(({ color }) => color !== undefined));
  }
});
