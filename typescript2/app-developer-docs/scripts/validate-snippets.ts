import { resolve } from 'node:path';

import { validateSnippetCatalog } from '../lib/snippets/checker';
import { parseOperatorArguments } from './operator-arguments';

const argumentsToUse = parseOperatorArguments(
  process.argv.slice(2),
  ['baml-bin'],
  [],
);
const binary =
  argumentsToUse.values.get('baml-bin') ?? process.env.BAML_BINARY ?? 'baml';
const appRoot = resolve(import.meta.dirname, '..');
const validation = await validateSnippetCatalog(binary, appRoot);

console.log(
  `Validated ${validation.results.length} BAML snippets with ${validation.toolchainVersion}.`,
);
for (const result of validation.results) {
  console.log(
    `- ${result.kind} ${result.id} (${result.pageSources.join(', ') || 'unreferenced'})`,
  );
}
