import assert from 'node:assert/strict';
import test from 'node:test';

import {
  findCliCommand,
  flattenCliCommands,
} from '../lib/generated-content/cli-routes.ts';
import { EXPLICIT_DOCUMENT_QUERY } from '../lib/generated-content/document-store.ts';
import { directRouteChildren } from '../lib/generated-content/routes.ts';
import type { CliCommandNodeInput } from '../lib/generated-content/schemas.ts';
import {
  canonicalVersionToRouteVersion,
  isPrereleaseVersion,
  routeVersionToCanonicalVersion,
} from '../lib/generated-content/versions.ts';

const leaf: CliCommandNodeInput = {
  arguments: [],
  command_path: ['generate', 'add'],
  description: 'Add a generator.',
  flags: [],
  name: 'add',
  subcommands: [],
  usage: 'baml generate add',
};
const root: CliCommandNodeInput = {
  arguments: [],
  command_path: [],
  description: null,
  flags: [],
  name: 'baml',
  subcommands: [
    {
      arguments: [],
      command_path: ['generate'],
      description: 'Generate clients.',
      flags: [],
      name: 'generate',
      subcommands: [leaf],
      usage: 'baml generate',
    },
  ],
  usage: 'baml',
};

test('exact-version routes validate and add only the required v prefix', () => {
  const canonical = '0.18.1-nightly.20260901.a';
  const routed = canonicalVersionToRouteVersion(canonical);
  assert.equal(routed, `v${canonical}`);
  assert.equal(routeVersionToCanonicalVersion(routed), canonical);
  assert.equal(routeVersionToCanonicalVersion(canonical), null);
  assert.equal(routeVersionToCanonicalVersion('v'), null);
  assert.equal(routeVersionToCanonicalVersion('v01.2.3'), null);
  assert.equal(routeVersionToCanonicalVersion('v1.2.3-alpha..1'), null);
  assert.equal(routeVersionToCanonicalVersion('v1.2.3-01'), null);
  assert.equal(
    routeVersionToCanonicalVersion('v1.2.3-alpha.1+linux-x86-64'),
    '1.2.3-alpha.1+linux-x86-64',
  );
  assert.equal(
    routeVersionToCanonicalVersion('v1.2.3-alpha.1+linux_x86_64'),
    null,
  );
  assert.throws(() => canonicalVersionToRouteVersion('../latest'));
  assert.equal(isPrereleaseVersion(canonical), true);
  assert.equal(isPrereleaseVersion('0.18.1'), false);
});

test('CLI routes mirror command tokens and reject unknown paths', () => {
  assert.deepEqual(
    flattenCliCommands(root).map((command) => command.command_path),
    [['generate'], ['generate', 'add']],
  );
  assert.equal(findCliCommand(root, ['generate', 'add']), leaf);
  assert.equal(findCliCommand(root, ['generate', 'remove']), null);
});

test('direct route children make hidden namespace descendants discoverable', () => {
  const pages = [
    { route_path: 'boundary/id' },
    { route_path: 'boundary/id/current' },
    { route_path: 'boundary/id/nested/value' },
    { route_path: 'boundary/other' },
  ];
  assert.deepEqual(directRouteChildren('boundary/id', pages), [pages[1]]);
});

test('explicit-version SSR uses one route query with exactly one snapshot join', () => {
  assert.equal((EXPLICIT_DOCUMENT_QUERY.match(/\bSELECT\b/gi) ?? []).length, 1);
  assert.equal((EXPLICIT_DOCUMENT_QUERY.match(/\bJOIN\b/gi) ?? []).length, 1);
  assert.match(EXPLICIT_DOCUMENT_QUERY, /routes\.version = \$1/);
  assert.match(EXPLICIT_DOCUMENT_QUERY, /routes\.path = \$2/);
  assert.match(EXPLICIT_DOCUMENT_QUERY, /doc_snapshots/);
});
