// lib/baml-runner/outbound.mjs
var WIRE_VARINT = 0;
var WIRE_I64 = 1;
var WIRE_LEN = 2;
var WIRE_I32 = 5;
var Reader = class {
  constructor(bytes) {
    this.bytesValue = bytes;
    this.index = 0;
  }
  get done() {
    return this.index >= this.bytesValue.length;
  }
  varint() {
    let result = 0n;
    let shift = 0n;
    for (; ; ) {
      if (this.done) throw new Error("truncated varint");
      const byte = this.bytesValue[this.index++];
      result |= BigInt(byte & 127) << shift;
      if ((byte & 128) === 0) return result;
      shift += 7n;
    }
  }
  bytes() {
    const length = Number(this.varint());
    if (this.index + length > this.bytesValue.length) {
      throw new Error("truncated length-delimited field");
    }
    const value = this.bytesValue.subarray(this.index, this.index + length);
    this.index += length;
    return value;
  }
  skip(wire) {
    if (wire === WIRE_VARINT) this.varint();
    else if (wire === WIRE_LEN) this.bytes();
    else if (wire === WIRE_I64) this.index += 8;
    else if (wire === WIRE_I32) this.index += 4;
    else throw new Error(`unsupported wire type ${wire}`);
  }
};
var textDecoder = new TextDecoder();
function visitFields(bytes, onField) {
  const reader = new Reader(bytes);
  while (!reader.done) {
    const tag = Number(reader.varint());
    const field = tag >>> 3;
    const wire = tag & 7;
    if (!onField(field, wire, reader)) reader.skip(wire);
  }
}
function mapEntry(bytes) {
  let key = "";
  let value = null;
  visitFields(bytes, (field, wire, reader) => {
    if (field === 1 && wire === WIRE_LEN) {
      key = textDecoder.decode(reader.bytes());
      return true;
    }
    if (field === 2 && wire === WIRE_LEN) {
      value = decodeOutboundValue(reader.bytes());
      return true;
    }
    return false;
  });
  return [key, value];
}
function repeatedInto(bytes, fieldNumber, decode) {
  const values = [];
  visitFields(bytes, (field, wire, reader) => {
    if (field === fieldNumber && wire === WIRE_LEN) {
      values.push(decode(reader.bytes()));
      return true;
    }
    return false;
  });
  return values;
}
function decodeOutboundValue(bytes) {
  let value;
  let matched = false;
  visitFields(bytes, (field, wire, reader) => {
    switch (field) {
      case 2:
        reader.bytes();
        value = null;
        matched = true;
        return true;
      case 3:
        value = textDecoder.decode(reader.bytes());
        matched = true;
        return true;
      case 4:
        value = BigInt.asIntN(64, reader.varint());
        matched = true;
        return true;
      case 5: {
        const view = new DataView(
          reader.bytesValue.buffer,
          reader.bytesValue.byteOffset + reader.index,
          8
        );
        value = view.getFloat64(0, true);
        reader.index += 8;
        matched = true;
        return true;
      }
      case 6:
        value = reader.varint() !== 0n;
        matched = true;
        return true;
      case 7: {
        const body = reader.bytes();
        let name = "";
        visitFields(body, (bodyField, bodyWire, bodyReader) => {
          if (bodyField === 1 && bodyWire === WIRE_LEN) {
            name = textDecoder.decode(bodyReader.bytes());
            return true;
          }
          return false;
        });
        value = {
          $baml: name,
          ...Object.fromEntries(repeatedInto(body, 2, mapEntry))
        };
        matched = true;
        return true;
      }
      case 8: {
        const body = reader.bytes();
        let enumValue = "";
        visitFields(body, (bodyField, bodyWire, bodyReader) => {
          if (bodyField === 2 && bodyWire === WIRE_LEN) {
            enumValue = textDecoder.decode(bodyReader.bytes());
            return true;
          }
          return false;
        });
        value = enumValue;
        matched = true;
        return true;
      }
      case 9: {
        const body = reader.bytes();
        visitFields(body, (literalField, literalWire, literalReader) => {
          if (literalField === 1 && literalWire === WIRE_LEN) {
            value = textDecoder.decode(literalReader.bytes());
            return true;
          }
          if (literalField === 2 && literalWire === WIRE_VARINT) {
            value = BigInt.asIntN(64, literalReader.varint());
            return true;
          }
          if (literalField === 3 && literalWire === WIRE_VARINT) {
            value = literalReader.varint() !== 0n;
            return true;
          }
          if (literalField === 4 && literalWire === WIRE_LEN) {
            value = BigInt(textDecoder.decode(literalReader.bytes()));
            return true;
          }
          if (literalField === 5 && literalWire === WIRE_LEN) {
            value = Number(textDecoder.decode(literalReader.bytes()));
            return true;
          }
          return false;
        });
        matched = true;
        return true;
      }
      case 11:
        value = repeatedInto(reader.bytes(), 2, decodeOutboundValue);
        matched = true;
        return true;
      case 12:
        value = Object.fromEntries(repeatedInto(reader.bytes(), 3, mapEntry));
        matched = true;
        return true;
      case 19:
        value = reader.bytes().slice();
        matched = true;
        return true;
      case 20:
        value = BigInt(textDecoder.decode(reader.bytes()));
        matched = true;
        return true;
      case 13:
      case 16:
      case 17:
      case 18:
      case 21:
      case 22:
        throw new Error(
          `BamlOutboundValue variant ${field} is not decoded yet; update the decoder from baml_outbound.proto`
        );
      default:
        return false;
    }
  });
  if (!matched) {
    throw new Error("BamlOutboundValue carried no recognized value variant");
  }
  return value;
}
function decodeBase64(base64) {
  if (typeof Buffer !== "undefined") {
    return new Uint8Array(Buffer.from(base64, "base64"));
  }
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}
function formatValue(value) {
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "bigint") return value.toString();
  if (value === null) return "null";
  if (value instanceof Uint8Array) {
    const bytes = [...value].map((byte) => `\\x${byte.toString(16).padStart(2, "0")}`).join("");
    return `b"${bytes}"`;
  }
  if (Array.isArray(value)) return `[${value.map(formatValue).join(", ")}]`;
  if (typeof value === "object") {
    const { $baml, ...rest } = value;
    const body = Object.entries(rest).map(([key, item]) => `${key}: ${formatValue(item)}`).join(", ");
    return $baml ? `${$baml} { ${body} }` : `{ ${body} }`;
  }
  return String(value);
}

// lib/baml-runner/result.mjs
var OUTBOUND_RENDERER = "baml.outbound.base64";
async function readRunResult({ boundaryId, outcome, readValue: readValue2 }) {
  if (outcome?.status !== "succeeded") {
    const message = outcome?.error?.message ?? `run ${outcome?.status ?? "failed"}`;
    throw new Error(message);
  }
  const result = outcome.result;
  if (!result) return null;
  if (result.rendererHint && result.rendererHint !== OUTBOUND_RENDERER) {
    throw new Error(`unsupported BAML result renderer: ${result.rendererHint}`);
  }
  if (typeof result.value === "string" && result.value.length > 0) {
    return decodeOutboundValue(decodeBase64(result.value));
  }
  if (result.valueRef) {
    if (typeof readValue2 !== "function") {
      throw new Error("the BAML result requires a value reader");
    }
    const body = await readValue2(boundaryId, result.valueRef);
    if (!body?.bodyBase64) {
      throw new Error(body?.diagnostic ?? "the BAML result value is unavailable");
    }
    return decodeOutboundValue(decodeBase64(body.bodyBase64));
  }
  return null;
}

// lib/baml-runner/driver.mjs
var TERMINAL = /* @__PURE__ */ new Set(["succeeded", "failed", "cancelled", "panicked"]);
function toPlain(value) {
  if (value instanceof Map) {
    return Object.fromEntries(
      [...value].map(([key, item]) => [String(key), toPlain(item)])
    );
  }
  if (Array.isArray(value)) return value.map(toPlain);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, toPlain(item)])
    );
  }
  return value;
}
var RunTimeout = class extends Error {
  constructor(milliseconds) {
    super(`the run did not finish within ${milliseconds}ms`);
    this.name = "RunTimeout";
  }
};
async function createSession(wasm, Vfs, files, options = {}) {
  const root = options.root ?? "/workspace";
  const vfs = new Vfs(root);
  vfs.setFiles(files);
  const lspPending = /* @__PURE__ */ new Map();
  const runListeners = /* @__PURE__ */ new Set();
  const valueWaiters = /* @__PURE__ */ new Map();
  let nextLspId = 0;
  let nextRequestId = 1;
  let projectId = null;
  let resolveProject;
  const projectReady = new Promise((resolve) => {
    resolveProject = resolve;
  });
  const unavailable = (capability) => () => {
    throw new Error(`${capability} is disabled for documentation examples`);
  };
  await wasm.start();
  const runtime = wasm.BamlWasmRuntime.create(
    {
      env: async () => void 0,
      fetch: unavailable("network access"),
      exec: unavailable("exec"),
      shell: unavailable("shell"),
      input: unavailable("input"),
      host_dispatch: unavailable("host functions"),
      lsp_make_request: () => {
      },
      lsp_send_notification: () => {
      },
      lsp_send_response: (raw) => {
        const response = toPlain(raw);
        const resolve = lspPending.get(response.id);
        if (resolve) {
          lspPending.delete(response.id);
          resolve(response);
        }
      },
      playground_send_notification: (raw) => {
        const notification = toPlain(raw);
        if (notification.type === "updateProject" && projectId === null) {
          projectId = notification.project;
          resolveProject(notification);
        }
        if (notification.type === "valueBody") {
          const resolve = valueWaiters.get(notification.valueRefId);
          if (resolve) {
            valueWaiters.delete(notification.valueRefId);
            resolve(notification);
          }
        }
        for (const listener of runListeners) listener(notification);
      }
    },
    vfs.wasmVfs
  );
  const lspRequest = (method, params) => new Promise((resolve) => {
    const id = nextLspId++;
    lspPending.set(id, resolve);
    runtime.handleLspRequest({ id, method, params });
  });
  await lspRequest("initialize", {
    capabilities: {
      textDocument: {
        publishDiagnostics: { relatedInformation: true },
        synchronization: { didSave: true, dynamicRegistration: true }
      },
      workspace: {}
    },
    processId: null,
    rootUri: `file://${root}`,
    workspaceFolders: [{ name: "docs-example", uri: `file://${root}` }]
  });
  runtime.handleLspNotification({ method: "initialized", params: {} });
  let documentVersion = 1;
  for (const [relativePath, text] of Object.entries(files)) {
    if (!relativePath.endsWith(".baml")) continue;
    runtime.handleLspNotification({
      method: "textDocument/didOpen",
      params: {
        textDocument: {
          languageId: "baml",
          text,
          uri: `file://${root}/${relativePath}`,
          version: documentVersion++
        }
      }
    });
  }
  runtime.requestPlaygroundState();
  const project = await projectReady;
  return {
    projectId,
    diagnostics: project?.update?.diagnostics ?? [],
    free: () => runtime.free(),
    async run(functionName, { timeoutMs = 3e4, signal } = {}) {
      const requestId = nextRequestId++;
      let boundaryId = null;
      let settle;
      const terminal = new Promise((resolve) => {
        settle = resolve;
      });
      const listener = (notification) => {
        if (notification.type === "runStarted" && notification.requestId === requestId) {
          boundaryId = notification.run?.boundaryId ?? null;
          if (TERMINAL.has(notification.run?.status)) {
            settle({ outcome: notification.run, boundaryId });
          }
        }
        if (notification.type === "commandError" && notification.requestId === requestId) {
          settle({
            boundaryId,
            outcome: {
              status: "failed",
              error: {
                class: notification.code,
                message: notification.message
              }
            }
          });
        }
        for (const change of notification.patch?.changes ?? []) {
          if (change.type === "complete") {
            settle({
              outcome: change.outcome,
              boundaryId: notification.patch.boundaryId
            });
          }
        }
      };
      runListeners.add(listener);
      const cancel = () => {
        if (!boundaryId) return;
        try {
          runtime.cancelRun(nextRequestId++, boundaryId);
        } catch {
        }
      };
      signal?.addEventListener("abort", cancel, { once: true });
      let timer;
      try {
        runtime.startRun(
          requestId,
          projectId,
          functionName,
          new Uint8Array(0)
        );
        const completed = await Promise.race([
          terminal,
          new Promise((resolve) => {
            timer = setTimeout(() => resolve("timeout"), timeoutMs);
          })
        ]);
        if (completed === "timeout") {
          cancel();
          throw new RunTimeout(timeoutMs);
        }
        const { boundaryId: completedBoundaryId, outcome } = completed;
        if (outcome?.status !== "succeeded") {
          return {
            status: outcome?.status ?? "failed",
            value: null,
            error: outcome?.error ?? null
          };
        }
        const value = await readRunResult({
          boundaryId: completedBoundaryId,
          outcome,
          readValue: (id, valueRef) => readValue(runtime, valueWaiters, () => nextRequestId++, id, valueRef, timeoutMs)
        });
        return { status: "succeeded", value, error: null };
      } finally {
        clearTimeout(timer);
        runListeners.delete(listener);
        signal?.removeEventListener("abort", cancel);
      }
    }
  };
}
function readValue(runtime, valueWaiters, nextId, boundaryId, valueRef, timeoutMs) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      valueWaiters.delete(valueRef.id);
      resolve(null);
    }, timeoutMs);
    valueWaiters.set(valueRef.id, (value) => {
      clearTimeout(timer);
      resolve(value);
    });
    queueMicrotask(() => runtime.readValue(nextId(), boundaryId, valueRef));
  });
}

// lib/baml-runner/vfs.mjs
var encoder = new TextEncoder();
var decoder = new TextDecoder();
var MEDIA_EXTENSIONS = /* @__PURE__ */ new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "svg",
  "webp",
  "ico",
  "bmp",
  "mp3",
  "wav",
  "ogg",
  "mp4",
  "webm",
  "pdf"
]);
function isMediaFile(path) {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return MEDIA_EXTENSIONS.has(ext);
}
var BamlVfs = class {
  constructor(rootPath) {
    this.files = /* @__PURE__ */ new Map();
    this.dirs = /* @__PURE__ */ new Set();
    this.suppressCallbacks = false;
    this.onChange = null;
    this.wasmVfs = {
      readDir: (path) => {
        const prefix = path.endsWith("/") ? path : path + "/";
        const children = /* @__PURE__ */ new Set();
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) {
            const rest = p.slice(prefix.length);
            const slash = rest.indexOf("/");
            children.add(slash >= 0 ? rest.slice(0, slash) : rest);
          }
        }
        for (const d of this.dirs) {
          if (d.startsWith(prefix)) {
            const rest = d.slice(prefix.length);
            if (rest && !rest.includes("/")) {
              children.add(rest);
            }
          }
        }
        return Array.from(children);
      },
      /** Single-round-trip directory listing with type info (added in canary).  */
      readDirEntries: (path) => {
        const prefix = path.endsWith("/") ? path : path + "/";
        const seen = /* @__PURE__ */ new Map();
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) {
            const rest = p.slice(prefix.length);
            const slash = rest.indexOf("/");
            if (slash >= 0) {
              seen.set(rest.slice(0, slash), "directory");
            } else if (!seen.has(rest)) {
              seen.set(rest, "file");
            }
          }
        }
        for (const d of this.dirs) {
          if (d.startsWith(prefix)) {
            const rest = d.slice(prefix.length);
            if (rest && !rest.includes("/") && !seen.has(rest)) {
              seen.set(rest, "directory");
            }
          }
        }
        return Array.from(seen.entries()).map(([name, file_type]) => ({
          name,
          file_type,
          is_symlink: false
        }));
      },
      createDir: (path) => {
        this.dirs.add(path);
        this.ensureParentDirs(path);
      },
      exists: (path) => {
        if (this.files.has(path) || this.dirs.has(path)) return true;
        const prefix = path + "/";
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) return true;
        }
        return false;
      },
      readFile: (path) => {
        const data = this.files.get(path);
        if (!data) throw new Error(`readFile: not found: ${path}`);
        return data;
      },
      writeFile: (path, data) => {
        this.files.set(path, data);
        this.ensureParentDirs(path);
        this.notifyWrite(path, data);
      },
      metadata: (path) => {
        if (this.files.has(path)) {
          return {
            file_type: "file",
            len: this.files.get(path).length,
            created: void 0,
            modified: void 0,
            accessed: void 0
          };
        }
        if (this.dirs.has(path)) {
          return {
            file_type: "directory",
            len: 0,
            created: void 0,
            modified: void 0,
            accessed: void 0
          };
        }
        const prefix = path + "/";
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) {
            return {
              file_type: "directory",
              len: 0,
              created: void 0,
              modified: void 0,
              accessed: void 0
            };
          }
        }
        throw new Error(`metadata: not found: ${path}`);
      },
      removeFile: (path) => {
        this.files.delete(path);
        this.notifyDelete(path);
      },
      removeDir: (path) => {
        this.dirs.delete(path);
        const prefix = path + "/";
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) {
            this.files.delete(p);
            this.notifyDelete(p);
          }
        }
        for (const d of this.dirs) {
          if (d.startsWith(prefix)) this.dirs.delete(d);
        }
      },
      setTime: (_type, _path, _time) => {
      },
      copyFile: (src, dest) => {
        const data = this.files.get(src);
        if (!data) throw new Error(`copyFile: source not found: ${src}`);
        const copy = new Uint8Array(data);
        this.files.set(dest, copy);
        this.ensureParentDirs(dest);
        this.notifyWrite(dest, copy);
      },
      moveFile: (src, dest) => {
        const data = this.files.get(src);
        if (!data) throw new Error(`moveFile: source not found: ${src}`);
        this.files.set(dest, data);
        this.files.delete(src);
        this.ensureParentDirs(dest);
        this.notifyDelete(src);
        this.notifyWrite(dest, data);
      },
      moveDir: (src, dest) => {
        const srcPrefix = src + "/";
        const entries = [];
        for (const [p, data] of this.files) {
          if (p.startsWith(srcPrefix)) {
            entries.push([dest + "/" + p.slice(srcPrefix.length), data]);
            this.files.delete(p);
            this.notifyDelete(p);
          }
        }
        for (const [p, data] of entries) {
          this.files.set(p, data);
          this.notifyWrite(p, data);
        }
        this.dirs.delete(src);
        this.dirs.add(dest);
      },
      readMany: (glob) => {
        const pattern = globToRegex(glob);
        const results = [];
        for (const [absPath, bytes] of this.files) {
          if (pattern.test(absPath)) results.push([absPath, bytes]);
        }
        return results;
      }
    };
    this.rootPath = rootPath;
    this.dirs.add(rootPath);
  }
  /** Convert a workspace-relative path to an absolute VFS path. */
  toAbsolute(relPath) {
    if (relPath.startsWith("/")) return relPath;
    return `${this.rootPath}/${relPath}`;
  }
  // -----------------------------------------------------------------------
  // Bulk updates from main thread
  // -----------------------------------------------------------------------
  /**
   * Replace all files. Keys are relative paths.
   * Text files (e.g. "baml_src/main.baml") have raw content strings.
   * Media files (e.g. "images/photo.png") have data-URL strings.
   */
  setFiles(files) {
    this.suppressCallbacks = true;
    try {
      const newAbsKeys = /* @__PURE__ */ new Set();
      for (const [rel, content] of Object.entries(files)) {
        const abs = this.toAbsolute(rel);
        newAbsKeys.add(abs);
        this.files.set(
          abs,
          isMediaFile(abs) ? dataUrlToBytes(content) : encoder.encode(content)
        );
        this.ensureParentDirs(abs);
      }
      for (const abs of this.files.keys()) {
        if (!newAbsKeys.has(abs)) {
          this.files.delete(abs);
        }
      }
    } finally {
      this.suppressCallbacks = false;
    }
  }
  // -----------------------------------------------------------------------
  // Internal helpers
  // -----------------------------------------------------------------------
  /** Convert an absolute VFS path back to a workspace-relative path. */
  toRelative(absPath) {
    const prefix = this.rootPath.endsWith("/") ? this.rootPath : this.rootPath + "/";
    if (absPath.startsWith(prefix)) return absPath.slice(prefix.length);
    return absPath;
  }
  /** Notify the main thread of a file write (text content or data URL). */
  notifyWrite(absPath, bytes) {
    if (this.suppressCallbacks || !this.onChange) return;
    const rel = this.toRelative(absPath);
    const content = isMediaFile(absPath) ? bytesToDataUrl(bytes, absPath) : decoder.decode(bytes);
    this.onChange({ path: rel, content });
  }
  /** Notify the main thread of a file deletion. */
  notifyDelete(absPath) {
    if (this.suppressCallbacks || !this.onChange) return;
    this.onChange({ path: this.toRelative(absPath), deleted: true });
  }
  ensureParentDirs(absPath) {
    let i = absPath.lastIndexOf("/");
    while (i > 0) {
      const dir = absPath.slice(0, i);
      if (this.dirs.has(dir)) break;
      this.dirs.add(dir);
      i = dir.lastIndexOf("/");
    }
  }
};
var MIME_TYPES = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  svg: "image/svg+xml",
  webp: "image/webp",
  ico: "image/x-icon",
  bmp: "image/bmp",
  mp3: "audio/mpeg",
  wav: "audio/wav",
  ogg: "audio/ogg",
  mp4: "video/mp4",
  webm: "video/webm",
  pdf: "application/pdf"
};
function bytesToDataUrl(bytes, path) {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const mime = MIME_TYPES[ext] ?? "application/octet-stream";
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return `data:${mime};base64,${btoa(binary)}`;
}
function globToRegex(glob) {
  let re = "^";
  let i = 0;
  while (i < glob.length) {
    if (glob[i] === "*" && glob[i + 1] === "*") {
      re += ".*";
      i += 2;
      if (glob[i] === "/") i++;
    } else if (glob[i] === "*") {
      re += "[^/]*";
      i++;
    } else if (glob[i] === "?") {
      re += "[^/]";
      i++;
    } else {
      re += glob[i].replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      i++;
    }
  }
  return new RegExp(re + "$");
}
function dataUrlToBytes(dataUrl) {
  const commaIdx = dataUrl.indexOf(",");
  if (commaIdx < 0) return encoder.encode(dataUrl);
  const base64 = dataUrl.slice(commaIdx + 1);
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

// lib/baml-runner/worker.mjs
var runtimePromise;
var sessions = /* @__PURE__ */ new Map();
async function sha256(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
async function projectKey(files) {
  const canonical = JSON.stringify(
    Object.entries(files).sort(([left], [right]) => left.localeCompare(right))
  );
  return sha256(new TextEncoder().encode(canonical));
}
async function loadRuntime() {
  const moduleStarted = performance.now();
  const manifestResponse = await fetch("/baml-runtime/manifest.json", {
    cache: "no-cache"
  });
  if (!manifestResponse.ok) {
    throw new Error(`runtime manifest returned HTTP ${manifestResponse.status}`);
  }
  const manifest = await manifestResponse.json();
  const runtimeModule = await import(manifest.module);
  const moduleLoadMs = performance.now() - moduleStarted;
  const downloadStarted = performance.now();
  const wasmResponse = await fetch(manifest.wasm, { cache: "force-cache" });
  if (!wasmResponse.ok) {
    throw new Error(`BAML runtime returned HTTP ${wasmResponse.status}`);
  }
  const wasmBytes = new Uint8Array(await wasmResponse.arrayBuffer());
  const wasmDownloadMs = performance.now() - downloadStarted;
  const actualDigest = await sha256(wasmBytes);
  if (actualDigest !== manifest.sha256) {
    throw new Error("BAML runtime digest does not match its manifest");
  }
  const initializationStarted = performance.now();
  await runtimeModule.default({ module_or_path: wasmBytes });
  const wasmInitializationMs = performance.now() - initializationStarted;
  return {
    manifest,
    wasm: runtimeModule,
    timings: { moduleLoadMs, wasmDownloadMs, wasmInitializationMs }
  };
}
async function getRuntime() {
  runtimePromise ??= loadRuntime().catch((error) => {
    runtimePromise = void 0;
    throw error;
  });
  return runtimePromise;
}
async function getSession(files) {
  const key = await projectKey(files);
  let pending = sessions.get(key);
  if (!pending) {
    pending = (async () => {
      const loaded = await getRuntime();
      const started = performance.now();
      const session = await createSession(loaded.wasm, BamlVfs, files, {
        root: `/docs-examples/${key}`
      });
      return {
        ...loaded,
        session,
        sessionInitializationMs: performance.now() - started
      };
    })().catch((error) => {
      sessions.delete(key);
      throw error;
    });
    sessions.set(key, pending);
  }
  return pending;
}
self.addEventListener("message", async (event) => {
  const { files, functionName = "main", id, type } = event.data ?? {};
  const respond = (message) => self.postMessage({ id, ...message });
  if (!id || !files || type !== "warm" && type !== "run") return;
  try {
    const ready = await getSession(files);
    const base = {
      manifest: {
        runtimeVersion: ready.manifest.runtimeVersion,
        sourceCommit: ready.manifest.sourceCommit
      },
      timings: {
        ...ready.timings,
        sessionInitializationMs: ready.sessionInitializationMs
      }
    };
    if (type === "warm") {
      respond({ ok: true, warmed: true, ...base });
      return;
    }
    const runStarted = performance.now();
    const result = await ready.session.run(functionName, { timeoutMs: 3e4 });
    const runMs = performance.now() - runStarted;
    if (result.status !== "succeeded") {
      respond({
        ok: false,
        error: result.error?.message ?? `run ${result.status}`,
        timings: { ...base.timings, runMs }
      });
      return;
    }
    respond({
      ok: true,
      output: formatValue(result.value),
      ...base,
      timings: { ...base.timings, runMs }
    });
  } catch (error) {
    respond({
      ok: false,
      error: error instanceof RunTimeout ? "The run took too long and was cancelled." : error?.message ?? String(error)
    });
  }
});
