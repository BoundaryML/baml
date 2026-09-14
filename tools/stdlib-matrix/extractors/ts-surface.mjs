// Extract the TypeScript-side comparison surface for the BAML stdlib matrix.
//
// Reads the repo-pinned `typescript` lib .d.ts files plus `@types/node`, and
// emits one JSON document of modules — `(globals)`, `(web)`, and one per node
// module — each holding the containers and free functions it declares, with
// member signatures printed verbatim from source. Declaration merging is
// handled by unioning members across lib files; the lib filename provides a
// free `since` facet (es5, es2015, …).
//
// The hierarchy is the reader's, not the compiler's. A developer looking for
// "how do I do this in BAML" reaches for a module first — `node:os`, or the
// globals — then a type inside it, then a member. So modules group, and both
// containers and free functions are records in their own right rather than
// mere headings: a container is a thing the matrix has an opinion about
// (`Date` corresponds to `baml.time.Instant`) and needs somewhere to put it.
//
// Deliberately signature-text-based, not type-model-based: the matrix compares
// surfaces semantically, so faithful printed signatures beat a lossy structural
// re-model. Output is deterministic: modules, containers and members sorted,
// inputs pinned by the lockfile.
//
// Usage: node tools/stdlib-matrix/extractors/ts-surface.mjs [--repo-root .]
// Prints the document to stdout.

import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import url from "node:url";

const FORMAT_VERSION = 2;

// ── scope ────────────────────────────────────────────────────────────────────
// The TS surface the matrix compares. One half of the scope definition; the
// BAML-side stdlib is the other. The lists are curated rather than exhaustive:
// the libs declare 1600-odd interfaces, most of them (WebGL, Intl, inspector)
// with no BAML counterpart, and asking a model to confirm that costs a call per
// container to learn nothing. Everything omitted is recorded in `scope` below
// so a reader can tell "not compared" from "missing".

// ECMAScript language builtins — `(globals)`.
const ECMA_CONTAINERS = new Set([
  // Primitives and their wrappers.
  "String", "Number", "BigInt", "Boolean", "Symbol",
  // Collections.
  "Array", "ReadonlyArray", "Map", "Set", "WeakMap", "WeakSet",
  // Binary.
  "ArrayBuffer", "SharedArrayBuffer", "DataView",
  "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array",
  "Int32Array", "Uint32Array", "Float32Array", "Float64Array",
  "BigInt64Array", "BigUint64Array",
  // Iteration. BAML's `baml.iter` and `baml.stream` answer for these.
  "Iterator", "IteratorObject", "AsyncIterator", "Generator", "AsyncGenerator",
  // Errors. BAML's `baml.errors` has counterparts for most.
  "Error", "TypeError", "RangeError", "SyntaxError", "EvalError",
  "ReferenceError", "URIError", "AggregateError",
  // The rest.
  "Object", "Function", "Promise", "Date", "RegExp",
  "JSON", "Math", "Reflect", "Atomics",
  "WeakRef", "FinalizationRegistry", "Proxy",
]);

const ECMA_FUNCTIONS = new Set([
  "parseInt", "parseFloat", "isNaN", "isFinite",
  "encodeURI", "encodeURIComponent", "decodeURI", "decodeURIComponent",
]);

// Web platform APIs that are global in browsers and in modern node — `(web)`.
// Split from the ECMAScript builtins because the two answer different
// questions: one is the language, the other is what the host lends it.
const WEB_CONTAINERS = new Set([
  // I/O and diagnostics.
  "Console", "Performance",
  // Network.
  "Request", "Response", "Headers", "FormData", "WebSocket",
  "URL", "URLSearchParams",
  // Binary and text.
  "Blob", "File", "TextEncoder", "TextDecoder",
  "TextEncoderStream", "TextDecoderStream",
  // Streams — `baml.stream`.
  "ReadableStream", "WritableStream", "TransformStream",
  "ReadableStreamDefaultReader", "WritableStreamDefaultWriter",
  "CompressionStream", "DecompressionStream",
  // Events and cancellation.
  "EventTarget", "Event", "CustomEvent", "AbortController", "AbortSignal",
  "MessageChannel", "MessagePort", "BroadcastChannel",
  // Crypto and errors.
  "Crypto", "SubtleCrypto", "DOMException",
]);

const WEB_FUNCTIONS = new Set([
  "fetch", "structuredClone", "atob", "btoa",
  "setTimeout", "setInterval", "clearTimeout", "clearInterval",
  "queueMicrotask", "reportError",
]);

// Constructor-interface twins (`interface StringConstructor`) hold the static
// side; fold them into their value container. Seeing one is also what tells us
// the container is a class rather than a bare interface.
const CONSTRUCTOR_SUFFIX = "Constructor";

// Namespace objects: single values whose members are reached on the object
// itself, with no constructor and no prototype to speak of. The lib declares
// them exactly as it declares a class's instance side (`interface JSON { … }`
// plus `declare var JSON: JSON`), so nothing in the declaration distinguishes
// them — but `JSON.prototype.parse` is not a thing, and an id that says
// otherwise is a wrong address.
//
// Some are spelled differently as a type and as a value: the interface is
// `Console` and the object is `console`. The value name is the one a reader
// writes, so it is the one the matrix uses.
const SINGLETONS = new Map([
  ["JSON", "JSON"],
  ["Math", "Math"],
  ["Reflect", "Reflect"],
  ["Atomics", "Atomics"],
  ["Console", "console"],
  ["Performance", "performance"],
]);

// Node modules, matched by `declare module "…"` name. Listed bare; the `node:`
// spelling of each is accepted too.
const NODE_MODULES = new Set([
  "assert", "buffer", "child_process", "crypto", "dns", "events", "fs",
  "fs/promises", "http", "https", "net", "os", "path", "process",
  "querystring", "readline", "stream", "timers", "tls", "url", "util",
  "worker_threads", "zlib",
]);

// The @types/node file each module is declared in. A module whose file is not
// where it was expected is recorded rather than skipped: without that, a
// reorganized @types/node makes a module's symbols vanish while the version
// string still claims they were compared, and a consumer cannot tell "matched
// nothing" from "never read".
const NODE_FILES = [
  "assert.d.ts", "buffer.d.ts", "child_process.d.ts", "crypto.d.ts",
  "dns.d.ts", "events.d.ts", "fs.d.ts", "fs/promises.d.ts", "http.d.ts",
  "https.d.ts", "net.d.ts", "os.d.ts", "path.d.ts", "process.d.ts",
  "querystring.d.ts", "readline.d.ts", "stream.d.ts", "timers.d.ts",
  "tls.d.ts", "url.d.ts", "util.d.ts", "worker_threads.d.ts", "zlib.d.ts",
];

// Some node modules expose their API as an interface on an exported value
// rather than as free function declarations (`path` → `PlatformPath`,
// `process` → `NodeJS.Process`). Pool those interfaces' members into the module
// itself, which is how a reader reaches them.
const NODE_MODULE_INTERFACES = new Map([
  ["path", new Set(["PlatformPath"])],
  ["process", new Set(["Process"])],
  ["util", new Set(["TextDecoder", "TextEncoder"])],
]);

// Interfaces worth keeping as containers of their own, per module. Node's
// declaration files name hundreds of options bags and callback shapes; these
// are the ones that describe a thing rather than an argument.
const NODE_CONTAINER_INTERFACES = new Map([
  ["buffer", new Set(["Buffer", "Blob", "File"])],
  ["fs", new Set(["Stats", "Dirent", "Dir"])],
  ["fs/promises", new Set(["FileHandle"])],
  ["http", new Set(["Agent", "Server", "IncomingMessage", "ServerResponse"])],
  ["net", new Set(["Socket", "Server"])],
  ["stream", new Set(["Readable", "Writable", "Duplex", "Transform"])],
  ["tls", new Set(["TLSSocket", "Server"])],
  ["url", new Set(["URL", "URLSearchParams"])],
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
  // @types/node absent: node modules simply don't appear; recorded below.
}

// ── extraction ───────────────────────────────────────────────────────────────

/** module name -> { name, kind, containers: Map, functions: Map } */
const modules = new Map();

function moduleFor(name, kind) {
  let m = modules.get(name);
  if (!m) {
    m = { name, kind, containers: new Map(), functions: new Map() };
    modules.set(name, m);
  }
  return m;
}

function containerFor(module, name, kind, since) {
  let c = module.containers.get(name);
  if (!c) {
    c = { name, kind, sources: new Set(), members: new Map(), doc: null, since };
    module.containers.set(name, c);
  }
  c.sources.add(since);
  // A container's kind is decided by the strongest evidence seen. A constructor
  // twin proves a class; a `declare var` proves a namespace object; a bare
  // interface declaration proves neither, and must not overwrite either.
  if (kind !== "interface") c.kind = kind;
  return c;
}

// The jsdoc description: every line up to the first `@tag`, stripped of comment
// markers. Tags themselves (`@param`, `@deprecated`, …) are dropped — they
// describe the signature, which the matrix already has structurally.
function jsdocDescription(node, sourceFile) {
  const ranges = ts.getLeadingCommentRanges(sourceFile.text, node.pos) ?? [];
  for (const range of ranges.reverse()) {
    const text = sourceFile.text.slice(range.pos, range.end);
    if (!text.startsWith("/**")) continue;
    const lines = [];
    for (const raw of text.split("\n")) {
      const line = raw
        .replace(/^\s*\/?\*+\/?\s?/, "")
        // A one-line jsdoc — `/** Returns the primitive value. */` — opens and
        // closes on the line the description is on, so stripping only the
        // leading marker left the terminator in the prose of 271 symbols.
        .replace(/\s*\*\/\s*$/, "")
        .trimEnd();
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

// A class body prints its whole implementation-free declaration, which for node
// classes runs to hundreds of lines. Only the head is a signature.
function declarationHead(node, sourceFile) {
  const text = signatureText(node, sourceFile);
  const brace = text.indexOf("{");
  return (brace < 0 ? text : text.slice(0, brace)).trim();
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

function addFunction(module, node, sourceFile, since) {
  const name = node.name?.text;
  if (!name) return;
  const sig = signatureText(node, sourceFile);
  let entry = module.functions.get(name);
  if (!entry) {
    entry = { name, signatures: [], doc: null, since };
    module.functions.set(name, entry);
  }
  if (!entry.signatures.includes(sig)) entry.signatures.push(sig);
  entry.doc ??= jsdocDescription(node, sourceFile);
}

// A member of an interface that *is* a module's API, recorded as one of the
// module's own entries. `memberName` rather than `node.name.text`, because
// these arrive as interface members and may be index or call signatures.
function addPooledMember(module, member, sourceFile) {
  const name = memberName(member, sourceFile);
  if (name === null) return;
  const sig = signatureText(member, sourceFile);
  let entry = module.functions.get(name);
  if (!entry) {
    entry = { name, signatures: [], doc: null, since: "node" };
    module.functions.set(name, entry);
  }
  if (!entry.signatures.includes(sig)) entry.signatures.push(sig);
  entry.doc ??= jsdocDescription(member, sourceFile);
}

function sinceOf(fileName) {
  const m = /lib\.(es\d+|es5|esnext|dom)/.exec(path.basename(fileName));
  return m ? m[1] : path.basename(fileName).replace(/^lib\.|\.d\.ts$/g, "");
}

// ── the lib files: `(globals)` and `(web)` ───────────────────────────────────

function walkLibFile(filePath, isDom) {
  const text = fs.readFileSync(filePath, "utf8");
  const sourceFile = ts.createSourceFile(filePath, text, ts.ScriptTarget.Latest, false);
  const since = sinceOf(filePath);
  const allowContainers = isDom ? WEB_CONTAINERS : ECMA_CONTAINERS;
  const allowFunctions = isDom ? WEB_FUNCTIONS : ECMA_FUNCTIONS;
  const moduleName = isDom ? "(web)" : "(globals)";
  const module = moduleFor(moduleName, isDom ? "web" : "globals");

  for (const stmt of sourceFile.statements) {
    if (ts.isInterfaceDeclaration(stmt)) {
      const declared = stmt.name.text;
      const base = declared.endsWith(CONSTRUCTOR_SUFFIX)
        ? declared.slice(0, -CONSTRUCTOR_SUFFIX.length)
        : declared;
      if (!allowContainers.has(declared) && !allowContainers.has(base)) continue;
      const isStatic = declared.endsWith(CONSTRUCTOR_SUFFIX) && allowContainers.has(base);
      // A constructor twin is proof of a class. Everything else stays an
      // interface until a `declare var` says otherwise.
      const name = SINGLETONS.get(base) ?? base;
      const container = containerFor(module, name, isStatic ? "class" : "interface", since);
      container.doc ??= jsdocDescription(stmt, sourceFile);
      for (const member of stmt.members) {
        addMember(container, member, sourceFile, { since, isStatic });
      }
    } else if (ts.isFunctionDeclaration(stmt) && stmt.name) {
      if (!allowFunctions.has(stmt.name.text)) continue;
      addFunction(module, stmt, sourceFile, since);
    } else if (ts.isVariableStatement(stmt)) {
      for (const decl of stmt.declarationList.declarations) {
        if (!ts.isIdentifier(decl.name) || !decl.type) continue;
        const valueName = decl.name.text;
        if (ts.isTypeReferenceNode(decl.type) && ts.isIdentifier(decl.type.typeName)) {
          // `declare var console: Console` — the value side of a namespace
          // object. Seeing it is what proves the container has no prototype.
          const typeName = decl.type.typeName.text;
          if (SINGLETONS.get(typeName) !== valueName) continue;
          if (!allowContainers.has(typeName)) continue;
          containerFor(module, valueName, "namespace", since);
        } else if (ts.isTypeLiteralNode(decl.type)) {
          // `declare var URL: { new (url: string): URL; prototype: URL; … }` —
          // the web platform's way of writing a constructor. The ECMAScript
          // libs use a named `*Constructor` interface, the DOM lib an anonymous
          // literal, and only the first was ever recognized: every web class
          // came out an interface with none of its statics.
          if (!allowContainers.has(valueName)) continue;
          // A namespace object's constructor is not a container of its own. The
          // DOM declares `interface Performance`, `declare var Performance: {…}`
          // *and* `declare var performance: Performance`; without this, the
          // middle one minted a second container holding nothing but
          // `prototype`, and `(web)` reported 34 containers for 33 names.
          if (SINGLETONS.has(valueName)) continue;
          const container = containerFor(module, valueName, "class", since);
          for (const member of decl.type.members) {
            addMember(container, member, sourceFile, { since, isStatic: true });
          }
        }
      }
    } else if (
      ts.isModuleDeclaration(stmt) &&
      ts.isIdentifier(stmt.name) &&
      stmt.body &&
      ts.isModuleBlock(stmt.body)
    ) {
      // `declare namespace Reflect { function get(…): any }`. A namespace
      // object that is not backed by an interface at all, so neither of the
      // branches above sees it.
      const name = stmt.name.text;
      if (!allowContainers.has(name)) continue;
      const container = containerFor(module, SINGLETONS.get(name) ?? name, "namespace", since);
      container.doc ??= jsdocDescription(stmt, sourceFile);
      for (const declared of stmt.body.statements) {
        if (ts.isFunctionDeclaration(declared) && declared.name) {
          addMember(container, declared, sourceFile, { since, isStatic: true });
        }
      }
    }
  }
}

// ── @types/node ──────────────────────────────────────────────────────────────

function walkNodeModuleFile(filePath) {
  const text = fs.readFileSync(filePath, "utf8");
  const sourceFile = ts.createSourceFile(filePath, text, ts.ScriptTarget.Latest, false);

  function visitModuleBody(bareName, body, nested = false) {
    const module = moduleFor(`node:${bareName}`, "node");
    const containerInterfaces = NODE_CONTAINER_INTERFACES.get(bareName);
    const pooledInterfaces = NODE_MODULE_INTERFACES.get(bareName);
    for (const stmt of body.statements ?? []) {
      if (ts.isFunctionDeclaration(stmt) && stmt.name) {
        // Only at the module's own level. Node declares overload helpers as
        // `export namespace readFile { function __promisify__(…) }`, 67 of them
        // in `fs` alone — pooling those upward invented `fs.__promisify__` and
        // `fs.native` as stdlib symbols (each judged by a model, at cost) while
        // merging 56 unrelated declarations under one id, destroying the real
        // `fs.readFile.__promisify__`.
        if (!nested) addFunction(module, stmt, sourceFile, "node");
      } else if (ts.isVariableStatement(stmt)) {
        // `export const EOL: string`. Reached on the module exactly as its
        // functions are, and dropped entirely until now — `os.EOL` and
        // `os.devNull` were both absent.
        //
        // `os.constants` stays out: it is a `namespace` of nested namespaces,
        // not a value, so there is no declaration to record. The comment that
        // used to sit below claimed `os.constants.signals` was reachable
        // through the module; it never was.
        if (!nested) {
          for (const decl of stmt.declarationList.declarations) {
            if (ts.isIdentifier(decl.name)) {
              addPooledMember(module, decl, sourceFile);
            }
          }
        }
      } else if (ts.isClassDeclaration(stmt) && stmt.name) {
        const container = containerFor(module, stmt.name.text, "class", "node");
        container.doc ??= jsdocDescription(stmt, sourceFile);
        for (const member of stmt.members) {
          addMember(container, member, sourceFile, {
            since: "node",
            isStatic: (ts.getCombinedModifierFlags(member) & ts.ModifierFlags.Static) !== 0,
          });
        }
      } else if (ts.isInterfaceDeclaration(stmt)) {
        const name = stmt.name.text;
        if (pooledInterfaces?.has(name)) {
          // The module's own API, declared as an interface on an exported value
          // (`path` → `PlatformPath`, `process` → `NodeJS.Process`). Its members
          // are reached on the module — `path.join`, `process.argv` — so they
          // are the module's functions, not a container's members. Wrapping
          // them in a pseudo-container would put a level in the tree that does
          // not exist in the code.
          for (const member of stmt.members) {
            addPooledMember(module, member, sourceFile);
          }
        } else if (containerInterfaces?.has(name)) {
          const container = containerFor(module, name, "interface", "node");
          container.doc ??= jsdocDescription(stmt, sourceFile);
          for (const member of stmt.members) {
            addMember(container, member, sourceFile, { since: "node", isStatic: false });
          }
        }
      } else if (ts.isModuleDeclaration(stmt) && stmt.body && ts.isModuleBlock(stmt.body)) {
        // Descended into for the interfaces it may hold — `process`'s API lives
        // in `declare namespace NodeJS { interface Process }` — but marked
        // nested, so the functions and values inside stay where they are
        // declared rather than being pooled onto the module.
        visitModuleBody(bareName, stmt.body, true);
      }
    }
  }

  for (const stmt of sourceFile.statements) {
    if (
      ts.isModuleDeclaration(stmt) &&
      ts.isStringLiteral(stmt.name) &&
      stmt.body &&
      ts.isModuleBlock(stmt.body)
    ) {
      const bare = stmt.name.text.replace(/^node:/, "");
      if (NODE_MODULES.has(bare)) visitModuleBody(bare, stmt.body);
    }
  }
}

// ── walking ──────────────────────────────────────────────────────────────────

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

const libFiles = fs
  .readdirSync(tsLibDir)
  .sort((a, b) => standardRank(a) - standardRank(b) || (a < b ? -1 : a > b ? 1 : 0));
for (const file of libFiles) {
  if (!file.startsWith("lib.") || !file.endsWith(".d.ts")) continue;
  walkLibFile(path.join(tsLibDir, file), file.startsWith("lib.dom"));
}

const missingNodeFiles = [];
if (nodeTypesDir) {
  for (const rel of NODE_FILES) {
    const p = path.join(nodeTypesDir, rel);
    if (fs.existsSync(p)) walkNodeModuleFile(p);
    else missingNodeFiles.push(rel);
  }
  if (missingNodeFiles.length === NODE_FILES.length) {
    console.error(`ts-surface: none of the expected @types/node files exist under ${nodeTypesDir}`);
    process.exit(2);
  }
}

// ── emit ─────────────────────────────────────────────────────────────────────

// Code-unit order, not `localeCompare`: that depends on the runtime's ICU data,
// and this file claims to be deterministic.
const byName = (a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);

const doc = {
  format_version: FORMAT_VERSION,
  typescript_version: tsPackage.version,
  types_node_version: nodeTypesVersion,
  scope: {
    globals: [...ECMA_CONTAINERS].sort(),
    global_functions: [...ECMA_FUNCTIONS].sort(),
    web: [...WEB_CONTAINERS].sort(),
    web_functions: [...WEB_FUNCTIONS].sort(),
    node_modules: [...NODE_MODULES].sort(),
    // Expected but absent. Empty is the normal case; a non-empty list says a
    // module's symbols are missing from this document rather than genuinely
    // having no counterpart.
    node_files_missing: missingNodeFiles.sort(),
  },
  modules: [...modules.values()]
    .map((m) => ({
      name: m.name,
      kind: m.kind,
      containers: [...m.containers.values()]
        .map((c) => ({
          name: c.name,
          kind: c.kind,
          singleton: c.kind === "namespace",
          doc: c.doc,
          since: c.since,
          sources: [...c.sources].sort(),
          members: [...c.members.values()]
            .map((member) => ({ ...member, signatures: [...member.signatures].sort() }))
            .sort((a, b) => byName(a, b) || Number(a.static) - Number(b.static)),
        }))
        .sort(byName),
      functions: [...m.functions.values()]
        .map((f) => ({ ...f, signatures: [...f.signatures].sort() }))
        .sort(byName),
    }))
    .sort(byName),
};

process.stdout.write(JSON.stringify(doc, null, 2) + "\n");
