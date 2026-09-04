import assert from 'node:assert/strict';
import test from 'node:test';

import {
  canonicalVersionToRouteVersion,
  routeVersionToCanonicalVersion,
} from '../lib/generated-content/build-content.ts';
import {
  findCliCommand,
  flattenCliCommands,
} from '../lib/generated-content/cli-routes.ts';
import type { CliCommandNodeInput } from '../lib/generated-content/schemas.ts';

const leaf: CliCommandNodeInput = {
  arguments: [],
  command_path: ['generate', 'add'],
  description: 'Add a generator.',
  flags: [],
  name: 'add',
  subcommands: [],
  usage: 'baml generate add',
};
const generate: CliCommandNodeInput = {
  arguments: [],
  command_path: ['generate'],
  description: 'Generate clients.',
  flags: [],
  name: 'generate',
  subcommands: [leaf],
  usage: 'baml generate',
};
const root: CliCommandNodeInput = {
  arguments: [],
  command_path: [],
  description: null,
  flags: [],
  name: 'baml',
  subcommands: [generate],
  usage: 'baml',
};

test('exact-version routes add and remove only the required v prefix', () => {
  const canonical = '0.18.1-nightly.20260901.a';
  const routed = canonicalVersionToRouteVersion(canonical);
  assert.equal(routed, `v${canonical}`);
  assert.equal(routeVersionToCanonicalVersion(routed), canonical);
  assert.equal(routeVersionToCanonicalVersion(canonical), null);
  assert.equal(routeVersionToCanonicalVersion('v'), null);
});

test('CLI routes mirror command tokens and reject unknown paths', () => {
  assert.deepEqual(
    flattenCliCommands(root).map((command) => command.command_path),
    [['generate'], ['generate', 'add']],
  );
  assert.equal(findCliCommand(root, ['generate', 'add']), leaf);
  assert.equal(findCliCommand(root, ['generate', 'remove']), null);
});
