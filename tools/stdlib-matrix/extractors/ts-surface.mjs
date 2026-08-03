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
  "Set", "JSON", "Math", "Date", "Promise", "PromiseConstructor", "RegExp",
  "Error", "Object", "Uint8Array", "ArrayBuffer", "Iterator", "Symbol",
]);

// Constructor-interface twins (`interface StringConstructor`) hold the
// static side; fold them into their value container.
const CONSTRUCTOR_SUFFIX = "Constructor";

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

function jsdocSummary(node, sourceFile) {
  const ranges = ts.getLeadingCommentRanges(sourceFile.text, node.pos) ?? [];
  for (const range of ranges.reverse()) {
    const text = sourceFile.text.slice(range.pos, range.end);
    if (!text.startsWith("/**")) continue;
    const line = text
      .split("\n")
      .map((l) => l.replace(/^\s*\/?\*+\/?\s?/, "").trim())
      .find((l) => l.length > 0 && !l.startsWith("@"));
    if (line) return line;
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
  return sourceFile.text.slice(name.pos, name.end).trim();
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
  entry.doc ??= jsdocSummary(member, sourceFile);
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

// ECMAScript core + DOM.
for (const file of fs.readdirSync(tsLibDir).sort()) {
  if (!file.startsWith("lib.") || !file.endsWith(".d.ts")) continue;
  const isDom = file.startsWith("lib.dom");
  walkLibFile(
    path.join(tsLibDir, file),
    isDom ? DOM_CONTAINERS : ECMA_CONTAINERS,
    isDom ? DOM_FUNCTIONS : new Set(),
  );
}

// Node.
if (nodeTypesDir) {
  for (const rel of ["fs/promises.d.ts", "child_process.d.ts", "process.d.ts", "net.d.ts", "path.d.ts"]) {
    const p = path.join(nodeTypesDir, rel);
    if (fs.existsSync(p)) walkNodeModuleFile(p);
  }
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
  },
  containers: [...containers.values()]
    .map((c) => ({
      name: c.name,
      kind: c.kind,
      sources: [...c.sources].sort(),
      members: [...c.members.values()]
        .map((m) => ({ ...m, signatures: [...m.signatures].sort() }))
        .sort((a, b) => a.name.localeCompare(b.name) || Number(a.static) - Number(b.static)),
    }))
    .sort((a, b) => a.name.localeCompare(b.name)),
};

process.stdout.write(JSON.stringify(doc, null, 2) + "\n");
