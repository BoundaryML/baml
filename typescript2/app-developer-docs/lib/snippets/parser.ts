import { load } from 'js-yaml';

import {
  type SnippetExpectation,
  snippetMetadataSchema,
  successfulSnippetExpectation,
} from './schema';

const metadataStartPattern = /^\s*\/\/ docs:meta\s*$/;
const metadataEndPattern = /^\s*\/\/ docs:endmeta\s*$/;
const regionStartPattern =
  /^\s*\/\/ docs:start ([A-Za-z0-9][A-Za-z0-9._-]*)\s*$/;
const regionEndPattern = /^\s*\/\/ docs:end ([A-Za-z0-9][A-Za-z0-9._-]*)\s*$/;
const docsDirectivePattern = /^\s*\/\/ docs:/;

export interface ParsedBamlSource {
  expectation: SnippetExpectation;
  hasMetadata: boolean;
  regions: ReadonlyMap<string, string>;
  source: string;
}

interface ParsedMetadataBlock {
  expectation: SnippetExpectation;
  hasMetadata: boolean;
  sourceLines: string[];
}

function fail(sourceName: string, lineNumber: number, message: string): never {
  throw new Error(`${sourceName}:${lineNumber}: ${message}`);
}

function parseMetadata(
  lines: readonly string[],
  sourceName: string,
): ParsedMetadataBlock {
  let metadataStart = -1;
  let metadataEnd = -1;

  for (const [index, line] of lines.entries()) {
    if (metadataStartPattern.test(line)) {
      if (metadataStart !== -1) {
        fail(sourceName, index + 1, 'only one docs metadata block is allowed');
      }
      metadataStart = index;
      continue;
    }
    if (metadataEndPattern.test(line)) {
      if (metadataStart === -1) {
        fail(sourceName, index + 1, 'docs:endmeta has no matching docs:meta');
      }
      if (metadataEnd !== -1) {
        fail(sourceName, index + 1, 'docs metadata block has multiple endings');
      }
      metadataEnd = index;
    }
  }

  if (metadataStart === -1) {
    return {
      expectation: successfulSnippetExpectation,
      hasMetadata: false,
      sourceLines: [...lines],
    };
  }
  if (metadataEnd === -1) {
    fail(
      sourceName,
      metadataStart + 1,
      'docs:meta has no matching docs:endmeta',
    );
  }
  if (metadataEnd < metadataStart) {
    fail(sourceName, metadataEnd + 1, 'docs:endmeta appears before docs:meta');
  }

  const yamlLines = lines
    .slice(metadataStart + 1, metadataEnd)
    .map((line, offset) => {
      const match = line.match(/^\s*\/\/( ?)(.*)$/);
      if (!match) {
        fail(
          sourceName,
          metadataStart + offset + 2,
          'every docs metadata line must begin with //',
        );
      }
      return match[2];
    });

  let expectation: SnippetExpectation;
  try {
    const metadata = snippetMetadataSchema.parse(load(yamlLines.join('\n')));
    expectation = metadata.expect;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    fail(sourceName, metadataStart + 1, `invalid docs metadata: ${detail}`);
  }

  return {
    expectation,
    hasMetadata: true,
    sourceLines: lines.filter(
      (_line, index) => index < metadataStart || index > metadataEnd,
    ),
  };
}

export function parseBamlSource(
  rawSource: string,
  sourceName: string,
): ParsedBamlSource {
  const normalizedSource = rawSource.replaceAll('\r\n', '\n');
  const { expectation, hasMetadata, sourceLines } = parseMetadata(
    normalizedSource.split('\n'),
    sourceName,
  );
  const regionLines = new Map<string, string[]>();
  const cleanedLines: string[] = [];
  let activeRegion: { name: string; startLine: number } | null = null;

  for (const [index, line] of sourceLines.entries()) {
    const startMatch = line.match(regionStartPattern);
    if (startMatch) {
      if (activeRegion) {
        fail(
          sourceName,
          index + 1,
          `region ${startMatch[1]} is nested inside ${activeRegion.name}`,
        );
      }
      if (regionLines.has(startMatch[1])) {
        fail(sourceName, index + 1, `duplicate region ${startMatch[1]}`);
      }
      activeRegion = { name: startMatch[1], startLine: index + 1 };
      regionLines.set(startMatch[1], []);
      continue;
    }

    const endMatch = line.match(regionEndPattern);
    if (endMatch) {
      if (!activeRegion) {
        fail(sourceName, index + 1, `region ${endMatch[1]} has no start`);
      }
      if (endMatch[1] !== activeRegion.name) {
        fail(
          sourceName,
          index + 1,
          `region ${endMatch[1]} closes active region ${activeRegion.name}`,
        );
      }
      activeRegion = null;
      continue;
    }

    if (docsDirectivePattern.test(line)) {
      fail(
        sourceName,
        index + 1,
        `malformed or unknown docs directive: ${line.trim()}`,
      );
    }

    cleanedLines.push(line);
    if (activeRegion) {
      regionLines.get(activeRegion.name)?.push(line);
    }
  }

  if (activeRegion) {
    fail(
      sourceName,
      activeRegion.startLine,
      `region ${activeRegion.name} has no matching end`,
    );
  }

  const source = cleanedLines.join('\n').trim();
  if (regionLines.size === 0) {
    regionLines.set('example', source.split('\n'));
  }

  return {
    expectation,
    hasMetadata,
    regions: new Map(
      [...regionLines].map(([name, content]) => [
        name,
        content.join('\n').trim(),
      ]),
    ),
    source,
  };
}
