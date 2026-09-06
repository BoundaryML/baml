import assert from 'node:assert/strict';
import test from 'node:test';

import {
  canonicalVersionToRouteVersion,
  type GeneratedReleaseSnapshot,
  type GeneratedReleaseSummary,
  isPrereleaseVersion,
  routeVersionToCanonicalVersion,
  selectFeaturedGeneratedRelease,
} from '../lib/generated-content/build-content.ts';
import {
  findCliCommand,
  flattenCliCommands,
} from '../lib/generated-content/cli-routes.ts';
import {
  generatedRoutePaths,
  generatedSearchEntries,
} from '../lib/generated-content/discovery.ts';
import { directRouteChildren } from '../lib/generated-content/routes.ts';
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

function releaseSummary(
  version: string,
  channels: GeneratedReleaseSummary['channels'],
): GeneratedReleaseSummary {
  return {
    channels,
    release: {
      created_at: new Date('2026-09-01T00:00:00Z'),
      generated_at: new Date('2026-09-01T00:00:00Z'),
      generator_version: '2b85a0ae8a258b7dbe21c346ffe05b051d62a08d',
      released_at: new Date('2026-09-01T00:00:00Z'),
      source_commit: '5a29ca428e3a964d262506ed90d889b3ab4d01a7',
      version,
    },
    routeVersion: `v${version}`,
  };
}

test('release discovery prefers stable and keeps prereleases explicit', () => {
  const nightly = releaseSummary('0.18.1-nightly.20260901.a', ['nightly']);
  const canary = releaseSummary('0.18.1-canary.1', ['canary']);
  const stable = releaseSummary('0.18.0', ['stable']);

  assert.equal(
    selectFeaturedGeneratedRelease([nightly, canary, stable]),
    stable,
  );
  assert.equal(selectFeaturedGeneratedRelease([nightly, canary]), canary);
  assert.equal(isPrereleaseVersion(nightly.release.version), true);
  assert.equal(isPrereleaseVersion(stable.release.version), false);
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

test('generated discovery covers exact routes and member search anchors', () => {
  const summary = releaseSummary('0.18.1-nightly.20260901.a', ['nightly']);
  const hash = '0'.repeat(64);
  const snapshot: GeneratedReleaseSnapshot = {
    channels: summary.channels,
    cli: {
      payload: {
        artifact_schema_version: 1,
        product_version: summary.release.version,
        raw_help: [
          {
            command_path: [],
            invocation: ['help'],
            sha256: hash,
            text: 'BAML help',
          },
        ],
        root,
        wrapper_version: '0.2.4',
      },
      row: {
        artifact_schema_version: 1,
        generated_at: new Date('2026-09-01T00:00:00Z'),
        payload_json: '{}',
        payload_sha256: hash,
        release_version: summary.release.version,
        source_sha256: hash,
        wrapper_version: '0.2.4',
      },
    },
    packages: [],
    pages: [
      {
        generated_at: new Date('2026-09-01T00:00:00Z'),
        package_export_id: 1,
        page_data: {
          cross_references: [],
          declaration: {
            id: 'V:boundary.id.current',
            kind: 'function',
            name: 'current',
          },
          display_name: 'current',
          exported_id: 'V:boundary.id.current',
          implementations: [],
          member_anchors: [
            {
              anchor: 'value',
              exported_id: 'F:boundary.id.current.value',
              label: 'value',
              member_kind: 'field',
            },
          ],
          namespace_path: ['id'],
          package_name: 'boundary',
          page_kind: 'function',
          qualified_name: 'boundary.id.current',
          schema_version: 1,
          summary: 'Returns the current identifier.',
        },
        page_kind: 'function',
        page_schema_version: 1,
        qualified_name: 'boundary.id.current',
        route_path: 'boundary/id/current',
      },
    ],
    release: summary.release,
    routeVersion: summary.routeVersion,
  };

  const paths = generatedRoutePaths(snapshot);
  assert.ok(paths.includes(`/baml/packages/${summary.routeVersion}`));
  assert.ok(
    paths.includes(
      `/baml/packages/${summary.routeVersion}/boundary/id/current`,
    ),
  );
  assert.ok(
    paths.includes(`/cli/${summary.routeVersion}/commands/generate/add`),
  );

  const search = generatedSearchEntries(summary, snapshot);
  assert.ok(
    search.some(
      (entry) =>
        entry.label === 'boundary.id.current.value' &&
        entry.href.endsWith('/boundary/id/current#value') &&
        entry.current,
    ),
  );
  assert.ok(search.some((entry) => entry.label === 'baml generate add'));
});
