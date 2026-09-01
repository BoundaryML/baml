import path from 'node:path';
import { buildBamlReferenceFiles } from './generate-baml-reference.mjs';
import { buildCliReferenceFiles } from './generate-cli-reference.mjs';

export function versionDirectory(version) {
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`Cannot generate routes for invalid BAML version ${JSON.stringify(version)}`);
  }
  return `v${version}`;
}

function addFiles(target, files, prefix = '') {
  for (const [name, contents] of files) {
    const output = prefix ? path.posix.join(prefix, name) : name;
    if (target.has(output)) throw new Error(`Duplicate generated file: ${output}`);
    target.set(output, contents);
  }
}

export function buildVersionedReferences(metadataEntries, defaultVersion) {
  const ordered = [...metadataEntries].sort((left, right) => (
    left.version === defaultVersion ? -1 : right.version === defaultVersion ? 1 : right.version.localeCompare(left.version)
  ));
  if (!ordered.some((entry) => entry.version === defaultVersion)) {
    throw new Error(`Default BAML docs version ${defaultVersion} was not loaded`);
  }

  const bamlContent = new Map();
  const bamlData = new Map();
  const cliContent = new Map();
  const cliData = new Map();
  const catalog = [];

  for (const metadata of ordered) {
    const directory = versionDirectory(metadata.version);
    const baml = buildBamlReferenceFiles(metadata);
    const cli = buildCliReferenceFiles(metadata);
    addFiles(bamlContent, baml.content, directory);
    addFiles(bamlData, baml.data, path.posix.join('versions', directory));
    addFiles(cliContent, cli.content, directory);
    addFiles(cliData, cli.data, path.posix.join('versions', directory));

    if (metadata.version === defaultVersion) {
      addFiles(bamlContent, baml.content);
      addFiles(bamlData, baml.data);
      addFiles(cliContent, cli.content);
      addFiles(cliData, cli.data);
    }

    catalog.push({
      version: metadata.version,
      channel: metadata.channel,
      releasedAt: metadata.releasedAt,
      sourceRevision: metadata.sourceRevision,
    });
  }

  return {
    bamlContent,
    bamlData,
    cliContent,
    cliData,
    catalog: {
      schemaVersion: 1,
      defaultVersion,
      versions: catalog,
    },
  };
}
