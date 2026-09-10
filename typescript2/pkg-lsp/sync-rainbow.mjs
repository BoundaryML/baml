import { readFileSync, writeFileSync } from 'node:fs';

const palette = JSON.parse(
  readFileSync(
    new URL(
      '../../baml_language/crates/baml_lsp/src/rainbow.json',
      import.meta.url,
    ),
    'utf8',
  ),
);
const manifestPath = new URL('../app-vscode-ext/package.json', import.meta.url);
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
manifest.contributes.semanticTokenTypes = palette.map(({ token }) => ({
  description: 'Native Rust sigil rainbow color.',
  id: token,
  superType: 'macro',
}));
const rules =
  manifest.contributes.configurationDefaults[
    'editor.semanticTokenColorCustomizations'
  ].rules;
for (const name of Object.keys(rules)) {
  if (/^bamlRainbow\d+:baml$/.test(name)) delete rules[name];
}
Object.assign(
  rules,
  Object.fromEntries(
    palette.map(({ token, color }) => [`${token}:baml`, color]),
  ),
);
manifest.contributes.configurationDefaults[
  'editor.semanticTokenColorCustomizations'
].rules = Object.fromEntries(
  Object.keys(rules)
    .sort(new Intl.Collator('en', { numeric: true }).compare)
    .map((key) => [key, rules[key]]),
);
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
