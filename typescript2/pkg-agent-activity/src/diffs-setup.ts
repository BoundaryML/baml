// One-time setup for @pierre/diffs: the BAML grammar and the options every
// diff in this app shares.

import bamlGrammar from '@b/pkg-grammar/baml.tmLanguage.json';
import { registerCustomLanguage } from '@pierre/diffs';

let registered = false;

export function ensureDiffsSetup() {
  if (registered) return;
  registered = true;
  // The loader must resolve to an array of grammars under `default`.
  registerCustomLanguage(
    'baml',
    async () => ({ default: [bamlGrammar as never] }),
    ['baml'],
  );
}
