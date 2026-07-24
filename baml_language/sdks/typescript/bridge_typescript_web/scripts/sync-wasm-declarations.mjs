import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const browserPath = resolve(packageRoot, "dist/wasm/bridge_web_core.d.ts");
const workerdPath = resolve(packageRoot, "dist/workerd-wasm/bridge_web_core.d.ts");
const sourcePath = resolve(packageRoot, "typescript_src/wasm/bridge_web_core.d.ts");
const check = process.argv.includes("--check");
const initializerNames = new Set(["InitInput", "InitOutput", "SyncInitInput", "initSync"]);

function normalizedNamedExports(path, source) {
  const sourceFile = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const printer = ts.createPrinter({ removeComments: true });
  const exports = new Map();

  for (const statement of sourceFile.statements) {
    const modifiers = statement.modifiers ?? [];
    if (!modifiers.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword)) continue;
    if (modifiers.some((modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword)) continue;

    const name = statement.name?.text;
    if (!name || initializerNames.has(name)) continue;
    if (!ts.isFunctionDeclaration(statement) && !ts.isClassDeclaration(statement) && !ts.isInterfaceDeclaration(statement) && !ts.isTypeAliasDeclaration(statement) && !ts.isEnumDeclaration(statement)) continue;

    const declaration = printer.printNode(ts.EmitHint.Unspecified, statement, sourceFile).replace(/\s+/g, " ").trim();
    exports.set(name, declaration);
  }

  return exports;
}

function diffExports(browser, workerd) {
  const names = [...new Set([...browser.keys(), ...workerd.keys()])].sort();
  const differences = [];
  for (const name of names) {
    const browserDeclaration = browser.get(name);
    const workerdDeclaration = workerd.get(name);
    if (browserDeclaration === workerdDeclaration) continue;
    differences.push(`${name}:\n  browser: ${browserDeclaration ?? "<missing>"}\n  workerd: ${workerdDeclaration ?? "<missing>"}`);
  }
  return differences;
}

const browserSource = readFileSync(browserPath, "utf8");
const workerdSource = readFileSync(workerdPath, "utf8");
const differences = diffExports(normalizedNamedExports(browserPath, browserSource), normalizedNamedExports(workerdPath, workerdSource));
if (differences.length > 0) throw new Error(`browser/workerd WASM declarations differ:\n${differences.join("\n")}`);

if (check) {
  const checkedInSource = readFileSync(sourcePath, "utf8");
  if (checkedInSource !== browserSource) throw new Error(`${sourcePath} is stale; run pnpm build:wasm`);
} else {
  writeFileSync(sourcePath, browserSource);
}
