// Extract the TypeScript-side comparison surface for the BAML stdlib matrix.
//
// Reads the repo-pinned `typescript` lib .d.ts files plus `@types/node`, and
// emits one JSON document of "containers" (String, Array, JSON, fs/promises,
// …) with their member signatures, printed verbatim from source. Declaration
// merging is handled by unioning members across lib files; the lib filename
// provides a free `since` facet (es5, es2015, …).
//
// Deliberately signature-text-based, not type-model-based: the matrix
// compares surfaces semantically (deterministic name matching + LLM judgment
// on the residue), so faithful printed signatures beat a lossy structural
// re-model. Output is deterministic: containers and members sorted, inputs
// pinned by the lockfile.
//
// Usage: node tools/stdlib-matrix/extractors/ts-surface.mjs [--repo-root .]
// Prints the document to stdout.

import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import url from "node:url";

const FORMAT_VERSION = 1;

// ── scope ────────────────────────────────────────────────────────────────────
// The TS containers the matrix compares. This is one half of the scope
// definition; the BAML-side mapping table is the other. Everything else in
// the libs is deliberately out of scope (recorded in the output so a reader
// can tell "not compared" from "missing").

const ECMA_CONTAINERS = new Set([
  "String", "Number", "BigInt", "Boolean", "Array", "ReadonlyArray", "Map",
  "Set", "JSON", "Math", "Date", "Promise", "RegExp",
  "Error", "Object", "Uint8Array", "ArrayBuffer", "Iterator", "Symbol",
]);

// Constructor-interface twins (`interface StringConstructor`) hold the
// static side; fold them into their value container.
const CONSTRUCTOR_SUFFIX = "Constructor";

// ECMAScript's namespace objects: single values whose members are reached on
// the object itself, with no constructor and no prototype to speak of. The lib
// declares them the same way it declares a class's instance side (`interface
// JSON { parse(...) }` plus `declare var JSON: JSON`), so nothing in the
// declaration distinguishes them — but `JSON.prototype.parse` is not a thing,
// and an id that says otherwise is a wrong address. The list is closed.
const NAMESPACE_OBJECTS = new Set(["JSON", "Math", "Reflect", "Atomics"]);

const DOM_CONTAINERS = new Set([
  "Blob", "File", "WebSocket", "Request", "Response", "Headers", "URL",
  "URLSearchParams", "AbortController", "AbortSignal", "TextEncoder",
  "TextDecoder", "Crypto", "SubtleCrypto",
]);

const DOM_FUNCTIONS = new Set(["fetch", "structuredClone", "atob", "btoa"]);

// Node modules, matched by `declare module "…"` name.
const NODE_MODULES = new Set([
  "fs/promises", "node:fs/promises", "child_process", "node:child_process",
  "process", "node:process", "net", "node:net", "path", "node:path",
]);

// Some node modules expose their API as an interface on an exported value
// rather than free function declarations (`path` → `PlatformPath`,
// `process` → `NodeJS.Process`). Pool those interfaces' members into the
// module container.
const NODE_MODULE_INTERFACES = new Map([
  ["path", new Set(["PlatformPath"])],
  ["process", new Set(["Process"])],
]);

// ── setup ────────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const rootFlag = args.indexOf("--repo-root");
if (rootFlag >= 0 && args[rootFlag + 1] === undefined) {
  // Otherwise `path.resolve(undefined)` throws, and the operator gets a stack
  // trace naming a function they did not call.
  console.error("ts-surface: --repo-root needs a path");
  process.exit(2);
}
const repoRoot = path.resolve(rootFlag >= 0 ? args[rootFlag + 1] : ".");

const require = createRequire(url.pathToFileURL(path.join(repoRoot, "package.json")));
const ts = require("typescript");
const tsPackage = require("typescript/package.json");
const tsLibDir = path.dirname(require.resolve("typescript/lib/typescript.js"));

let nodeTypesVersion = null;
let nodeTypesDir = null;
try {
  nodeTypesDir = path.dirname(require.resolve("@types/node/package.json"));
  nodeTypesVersion = require("@types/node/package.json").version;
} catch {
  // @types/node absent: node containers simply don't appear; recorded below.
}

// ── extraction ───────────────────────────────────────────────────────────────

/** container name -> { kind, sources:Set, members: Map<key, member> } */
const containers = new Map();

function containerFor(name, kind, source) {
  let c = containers.get(name);
  if (!c) {
    c = { name, kind, sources: new Set(), members: new Map() };
    containers.set(name, c);
  }
  c.sources.add(source);
  return c;
}

// The jsdoc description: every line up to the first `@tag`, stripped of
// comment markers. Tags themselves (`@param`, `@deprecated`, …) are dropped —
// they describe the signature, which the matrix already has structurally.
function jsdocDescription(node, sourceFile) {
  const ranges = ts.getLeadingCommentRanges(sourceFile.text, node.pos) ?? [];
  for (const range of ranges.reverse()) {
    const text = sourceFile.text.slice(range.pos, range.end);
    if (!text.startsWith("/**")) continue;
    const lines = [];
    for (const raw of text.split("\n")) {
      const line = raw.replace(/^\s*\/?\*+\/?\s?/, "").trimEnd();
      if (line.trimStart().startsWith("@")) break;
      lines.push(line);
    }
    const description = lines.join("\n").trim();
    if (description) return description;
  }
  return null;
}

function signatureText(member, sourceFile) {
  // Verbatim, minus jsdoc, collapsed whitespace — faithful and diffable.
  return member
    .getText(sourceFile)
    .replace(/\/\*\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ")
    .replace(/;$/, "")
    .trim();
}

function memberName(member, sourceFile) {
  const name = member.name;
  if (!name) {
    if (ts.isCallSignatureDeclaration(member)) return "(call)";
    if (ts.isConstructSignatureDeclaration(member)) return "(new)";
    if (ts.isIndexSignatureDeclaration(member)) return "(index)";
    return null;
  }
  if (ts.isIdentifier(name) || ts.isStringLiteral(name)) return name.text;
  // Computed / exotic names (e.g. `[Symbol.iterator]`): verbatim slice.
  // `getStart` rather than `pos`, which begins at the leading trivia and would
  // fold the member's own jsdoc into its name.
  return sourceFile.text.slice(name.getStart(sourceFile), name.end).replace(/\s+/g, " ").trim();
}

function addMember(container, member, sourceFile, { since, isStatic }) {
  const name = memberName(member, sourceFile);
  if (name === null) return;
  const sig = signatureText(member, sourceFile);
  // Merge overloads under one member entry; keep every distinct signature.
  const key = `${isStatic ? "static " : ""}${name}`;
  let entry = container.members.get(key);
  if (!entry) {
    entry = {
      name,
      static: isStatic,
      signatures: [],
      doc: null,
      since,
      optional: Boolean(member.questionToken),
    };
    container.members.set(key, entry);
  }
  if (!entry.signatures.includes(sig)) entry.signatures.push(sig);
  entry.doc ??= jsdocDescription(member, sourceFile);
}

function sinceOf(fileName) {
  const m = /lib\.(es\d+|es5|esnext|dom)/.exec(path.basename(fileName));
  return m ? m[1] : path.basename(fileName).replace(/^lib\.|\.d\.ts$/g, "");
}

function walkLibFile(filePath, allowInterfaces, allowFunctions) {
  const text = fs.readFileSync(filePath, "utf8");
  const sourceFile = ts.createSourceFile(filePath, text, ts.ScriptTarget.Latest, false);
  const since = sinceOf(filePath);

  for (const stmt of sourceFile.statements) {
    if (ts.isInterfaceDeclaration(stmt)) {
      const name = stmt.name.text;
      const base = name.endsWith(CONSTRUCTOR_SUFFIX)
        ? name.slice(0, -CONSTRUCTOR_SUFFIX.length)
        : name;
      if (!allowInterfaces.has(name) && !allowInterfaces.has(base)) continue;
      const isStatic = name.endsWith(CONSTRUCTOR_SUFFIX) && allowInterfaces.has(base);
      const container = containerFor(isStatic ? base : name, "interface", since);
      for (const member of stmt.members) {
        addMember(container, member, sourceFile, { since, isStatic });
      }
    } else if (ts.isFunctionDeclaration(stmt) && stmt.name) {
      if (!allowFunctions.has(stmt.name.text)) continue;
      const container = containerFor("(globals)", "functions", since);
      addMember(container, stmt, sourceFile, { since, isStatic: false });
    }
  }
}

function walkNodeModuleFile(filePath) {
  const text = fs.readFileSync(filePath, "utf8");
  const sourceFile = ts.createSourceFile(filePath, text, ts.ScriptTarget.Latest, false);

  function visitModuleBody(moduleName, body) {
    const bareName = moduleName.replace(/^node:/, "");
    for (const stmt of body.statements ?? []) {
      if (ts.isFunctionDeclaration(stmt) && stmt.name) {
        const container = containerFor(`node:${bareName}`, "module", "node");
        addMember(container, stmt, sourceFile, { since: "node", isStatic: false });
      } else if (
        ts.isInterfaceDeclaration(stmt) &&
        NODE_MODULE_INTERFACES.get(bareName)?.has(stmt.name.text)
      ) {
        const container = containerFor(`node:${bareName}`, "module", "node");
        for (const member of stmt.members) {
          addMember(container, member, sourceFile, { since: "node", isStatic: false });
        }
      } else if (ts.isModuleDeclaration(stmt) && stmt.body && ts.isModuleBlock(stmt.body)) {
        visitModuleBody(moduleName, stmt.body);
      }
    }
  }

  for (const stmt of sourceFile.statements) {
    if (
      ts.isModuleDeclaration(stmt) &&
      ts.isStringLiteral(stmt.name) &&
      NODE_MODULES.has(stmt.name.text) &&
      stmt.body &&
      ts.isModuleBlock(stmt.body)
    ) {
      visitModuleBody(stmt.name.text, stmt.body);
    }
  }
}

// The standard a member was introduced by, as a sort key.
//
// `since` is recorded when a member is first seen and never revised, so the
// visit order decides it. Lexicographic order gets this wrong: "lib.dom" and
// "lib.es2015.*" both sort before "lib.es5.d.ts" — "2" precedes "5" — and the
// libs re-declare members across files through declaration merging, so
// `Object.keys` came out as es2015 when ES5 introduced it. Oldest standard
// first makes the first sighting the earliest one.
function standardRank(file) {
  if (file.startsWith("lib.es5.")) return 5;
  const year = /^lib\.es(\d{4})\./.exec(file);
  if (year) return Number(year[1]);
  if (file.startsWith("lib.esnext")) return 9998;
  // DOM and the rest describe no ECMAScript edition; they go last so an ES
  // member is never attributed to them.
  return 9999;
}

// ECMAScript core + DOM.
const libFiles = fs
  .readdirSync(tsLibDir)
  .sort((a, b) => standardRank(a) - standardRank(b) || (a < b ? -1 : a > b ? 1 : 0));
for (const file of libFiles) {
  if (!file.startsWith("lib.") || !file.endsWith(".d.ts")) continue;
  const isDom = file.startsWith("lib.dom");
  walkLibFile(
    path.join(tsLibDir, file),
    isDom ? DOM_CONTAINERS : ECMA_CONTAINERS,
    isDom ? DOM_FUNCTIONS : new Set(),
  );
}

// Node. A file that is not where it was expected is recorded rather than
// skipped: without that, a reorganized @types/node makes a module's symbols
// vanish while the version string still claims they were compared, and a
// consumer cannot tell "matched nothing" from "never read".
const missingNodeFiles = [];
if (nodeTypesDir) {
  for (const rel of ["fs/promises.d.ts", "child_process.d.ts", "process.d.ts", "net.d.ts", "path.d.ts"]) {
    const p = path.join(nodeTypesDir, rel);
    if (fs.existsSync(p)) walkNodeModuleFile(p);
    else missingNodeFiles.push(rel);
  }
}
if (nodeTypesDir && missingNodeFiles.length === 5) {
  console.error(`ts-surface: none of the expected @types/node files exist under ${nodeTypesDir}`);
  process.exit(2);
}

// ── emit ─────────────────────────────────────────────────────────────────────

const doc = {
  format_version: FORMAT_VERSION,
  typescript_version: tsPackage.version,
  types_node_version: nodeTypesVersion,
  scope: {
    ecma: [...ECMA_CONTAINERS].sort(),
    dom: [...DOM_CONTAINERS].sort(),
    dom_functions: [...DOM_FUNCTIONS].sort(),
    node_modules: [...NODE_MODULES].filter((m) => !m.startsWith("node:")).sort(),
    // Expected but absent. Empty is the normal case; a non-empty list says a
    // module's symbols are missing from this document rather than genuinely
    // having no counterpart.
    node_files_missing: missingNodeFiles.sort(),
  },
  containers: [...containers.values()]
    .map((c) => ({
      name: c.name,
      kind: c.kind,
      singleton: NAMESPACE_OBJECTS.has(c.name),
      sources: [...c.sources].sort(),
      members: [...c.members.values()]
        .map((m) => ({ ...m, signatures: [...m.signatures].sort() }))
        // Code-unit order, not `localeCompare`: that depends on the runtime's
        // ICU data, and this file claims to be deterministic.
        .sort(
          (a, b) =>
            (a.name < b.name ? -1 : a.name > b.name ? 1 : 0) ||
            Number(a.static) - Number(b.static),
        ),
    }))
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)),
};

process.stdout.write(JSON.stringify(doc, null, 2) + "\n");
