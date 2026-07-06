#!/usr/bin/env tsx
// Generate the canonical TextMate grammar JSON from the typed sources in src/.
// The emitted JSON files are the artifacts every consumer reads
// (app-promptfiddle, pkg-editor) and the source for the app-vscode-ext mirror
// (see scripts/sync.mjs). Run via `pnpm --filter @b/pkg-grammar build`.

import { writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { emitJSON, type Grammar } from "tmlanguage-generator";
import { baml } from "../src/baml.ts";

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(here, "..");

const GRAMMARS: { grammar: Grammar; out: string }[] = [
  { grammar: baml, out: "baml.tmLanguage.json" },
];

for (const { grammar, out } of GRAMMARS) {
  const json = await emitJSON(grammar, { errorSourceFilePath: out });
  writeFileSync(resolve(pkgRoot, out), json);
  console.log(`generated ${out}`);
}
