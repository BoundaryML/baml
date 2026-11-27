// The native bridge's runtime: what generated TypeScript SDKs import as
// `@boundaryml/baml-bridge`, over a BAML program compiled to a native
// library (the addon in `../src`).
//
// It exports the names today's bridge exports. Free functions (`defineFunction`,
// async and sync), the type map, and the error classes are real; everything
// else (instance methods, media, streams, tracing, per-call options) is a stub
// that throws when it is used, so a generated SDK imports cleanly.
//
// How calls run: BAML runs on Node's main thread, inside the addon. A call
// either starts and runs a turn at once (the "eager" turn, default) or starts
// and schedules one turn by `queueMicrotask` (`BAML_NATIVE_TURN=microtask`),
// so that calls started together share a turn. A turn returns what ended,
// copied into JavaScript, and when the next BAML timer fires; the library's
// wake (its I/O thread) arrives through a thread-safe function. A `_sync`
// function drives turns on the spot until its call ends.

import { fileURLToPath } from "node:url";

// ─── errors: the same classes, names and fields as today's bridge ───

export class BamlError extends Error {
  constructor(message, detail) {
    super(message);
    this.name = "BamlError";
    this.value = detail?.value;
    this.bamlTrace = detail?.bamlTrace ? [...detail.bamlTrace] : [];
    this.className = detail?.className;
  }
}

export class BamlInvalidArgumentError extends BamlError {
  constructor(message, detail) {
    super(message, detail);
    this.name = "BamlInvalidArgumentError";
  }
}

export class BamlClientError extends BamlError {
  constructor(message, detail) {
    super(message, detail);
    this.name = "BamlClientError";
  }
}

export class BamlCancelledError extends BamlError {
  constructor(message, detail) {
    super(message, detail);
    this.name = "BamlCancelledError";
  }
}

export class BamlAbortError extends Error {
  constructor(message, options) {
    super(message);
    this.name = "AbortError";
    this.reason = options?.reason;
  }
}

export class BamlPanic extends BamlError {
  constructor(message, detail) {
    super(message, detail);
    this.name = "BamlPanic";
  }
}

export function wrapNativeError(err) {
  if (err instanceof BamlError) return err;
  const message = err instanceof Error ? err.message : String(err);
  const code = err !== null && typeof err === "object" && "code" in err ? err.code : undefined;
  if (code === "invalid_argument") return new BamlInvalidArgumentError(message);
  return new BamlClientError(message);
}

// ─── the type map, as today's bridge has it ───

export class BamlTypeMap {
  constructor() {
    this.classLazy = new Map();
    this.enumLazy = new Map();
    this.aliasLazy = new Map();
    this.cache = new Map();
    this._descriptor = null;
  }

  static fromLazyEntries(args) {
    const m = new BamlTypeMap();
    for (const [fqn, le] of Object.entries(args.classes)) m.classLazy.set(fqn, le);
    for (const [fqn, le] of Object.entries(args.enums)) m.enumLazy.set(fqn, le);
    for (const [fqn, le] of Object.entries(args.typeAliases)) m.aliasLazy.set(fqn, le);
    // The native library carries its own interface; the descriptor is unused.
    m._descriptor = args.descriptor ?? null;
    return m;
  }

  get descriptor() {
    return this._descriptor;
  }

  _resolve(fqn, lazy, kind) {
    const key = `${kind}:${fqn}`;
    if (this.cache.has(key)) return this.cache.get(key);
    const thunk = lazy.get(fqn);
    if (thunk === undefined) throw new BamlError(`Unknown ${kind} FQN ${fqn}`);
    const resolved = thunk();
    if (resolved === undefined) {
      throw new BamlError(`Could not resolve ${kind} ${fqn} (resolver returned undefined)`);
    }
    this.cache.set(key, resolved);
    return resolved;
  }

  getClass(fqn) {
    return this._resolve(fqn, this.classLazy, "class");
  }

  getEnum(fqn) {
    return this._resolve(fqn, this.enumLazy, "enum");
  }

  getTypeAlias(fqn) {
    return this._resolve(fqn, this.aliasLazy, "type alias");
  }

  warm() {
    for (const k of this.classLazy.keys()) this.getClass(k);
    for (const k of this.enumLazy.keys()) this.getEnum(k);
  }

  _ensureReverse() {
    if (this.reverse === undefined) {
      this.reverse = new Map();
      for (const [fqn, thunk] of [...this.classLazy, ...this.enumLazy]) {
        try {
          this.reverse.set(thunk(), fqn);
        } catch {
          /* unresolvable entry: skip */
        }
      }
    }
  }

  captureType(ctor) {
    this._ensureReverse();
    return this.reverse.get(ctor) ?? "";
  }

  jsTypeToBamlType(ctor) {
    this._ensureReverse();
    for (let cur = ctor; cur != null; cur = Object.getPrototypeOf(cur)) {
      const fqn = this.reverse.get(cur);
      if (fqn !== undefined) return fqn;
    }
    return "";
  }
}

let typeMap = new BamlTypeMap();

export function setTypeMap(m) {
  typeMap = m;
  bound = false;
}

export function getTypeMap() {
  return typeMap;
}

// ─── the addon ───

const addonPath =
  process.env.BAML_NATIVE_NODE_ADDON ??
  fileURLToPath(new URL("../../../target/release/libbaml_native_node.dylib", import.meta.url));
const addonModule = { exports: {} };
process.dlopen(addonModule, addonPath);
const addon = addonModule.exports;

/** How a class instance is built: `props`, `positional` or `init`. */
const BUILD = process.env.BAML_NATIVE_BUILD ?? "positional";
const BUILD_MODE = { props: 0, positional: 1, init: 2 }[BUILD] ?? 1;
/** When a turn runs after a call starts: `eager` or `microtask`. */
const EAGER = (process.env.BAML_NATIVE_TURN ?? "eager") !== "microtask";

let loaded = false;
let bound = false;
/** Per export name: { index, params, defaults }. */
const exportsByName = new Map();

/**
 * Load the program's library and check that it is the one the bindings were
 * generated for. Generated `index.ts` calls it in place of
 * `initializeRuntimeFromBlob`.
 */
export function initializeNativeLibrary(path, interfaceHash) {
  addon.load(path, interfaceHash, scheduleTurn);
  loaded = true;
  exportsByName.clear();
  addon.functions().forEach(([name, params, defaults], index) => {
    exportsByName.set(name, { index, params, defaults });
  });
}

const EMPTY = Object.freeze({});

/** Hand the addon each interface type's generated class or enum. */
function bindTypes() {
  const types = addon.types();
  const constructors = [];
  const factories = [];
  const enumValues = [];
  for (const [name, isEnum, names] of types) {
    if (isEnum) {
      let En;
      try {
        En = typeMap.getEnum(`user.${name}`);
      } catch {
        En = undefined;
      }
      constructors.push(undefined);
      factories.push(undefined);
      enumValues.push(names.map((v) => (En !== undefined && v in En ? En[v] : v)));
      continue;
    }
    let C;
    try {
      C = typeMap.getClass(`user.${name}`);
    } catch {
      C = undefined;
    }
    constructors.push(C);
    enumValues.push(undefined);
    factories.push(C === undefined ? undefined : makeFactory(C, names));
  }
  addon.bind(constructors, factories, enumValues, () => Object.create(null), BUILD_MODE);
  bound = true;
}

/** A per-class function taking the field values in declaration order. */
function makeFactory(C, names) {
  const params = names.map((_, i) => `v${i}`).join(", ");
  if (BUILD === "init") {
    const init = names.map((n, i) => `${JSON.stringify(n)}: v${i}`).join(", ");
    return new Function("C", `return function (${params}) { return new C({ ${init} }); };`)(C);
  }
  const sets = names.map((n, i) => `o[${JSON.stringify(n)}] = v${i};`).join(" ");
  return new Function(
    "C",
    "E",
    `return function (${params}) { const o = new C(E); ${sets} return o; };`,
  )(C, EMPTY);
}

function ready() {
  if (!loaded) throw new BamlPanic("the BAML library is not loaded: import the generated SDK first");
  if (!bound) bindTypes();
}

// ─── calls and turns ───

const STATUS_RETURNED = 0;
const STATUS_THREW = 1;
const STATUS_STUCK = 2;
const STATUS_PANICKED = 3;
const STATUS_UNHANDLED = 4;
const STATUS_HOST_THREW = 5;
const STATUS_DECODE = 6;

/** Pending async calls: id -> { resolve, reject }. */
const pending = new Map();
/** The call a `_sync` function drives, while it does. */
let syncSlot = null;
let turnScheduled = false;
let timer = null;

function scheduleTurn() {
  if (!turnScheduled) {
    turnScheduled = true;
    queueMicrotask(runTurn);
  }
}

function runTurn() {
  turnScheduled = false;
  if (!bound) {
    if (!loaded) return;
    bindTypes();
  }
  deliver(addon.turn());
}

function onTimer() {
  timer = null;
  runTurn();
}

/** Settle every call a turn reports, and arm the timer it asks for. */
function deliver(events) {
  for (let i = 2; i < events.length; i += 3) settle(events[i], events[i + 1], events[i + 2]);
  armTimer(events[0], events[1]);
}

/**
 * One timer for the next BAML timer. It keeps the process alive only while
 * calls are pending: with none, it is unreferenced, and fires if the process
 * is still running.
 */
function armTimer(delay, pendingCount) {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  if (delay >= 0) {
    timer = setTimeout(onTimer, Math.ceil(delay));
    if (pendingCount === 0) timer.unref();
  }
}

function settle(id, status, payload) {
  if (status === STATUS_UNHANDLED) {
    // As today's bridge: a failure nobody awaits is thrown on the loop.
    const error = thrownError("error", payload);
    queueMicrotask(() => {
      throw error;
    });
    return;
  }
  if (syncSlot !== null && syncSlot.id === id) {
    syncSlot.status = status;
    syncSlot.payload = payload;
    syncSlot.done = true;
    return;
  }
  const waiter = pending.get(id);
  if (waiter === undefined) return; // nobody waits: the result is dropped
  pending.delete(id);
  if (status === STATUS_RETURNED) waiter.resolve(payload);
  else waiter.reject(callError(status, payload));
}

function callError(status, payload) {
  switch (status) {
    case STATUS_THREW:
      return thrownError("error", payload);
    case STATUS_PANICKED:
      return thrownError("panic", payload);
    case STATUS_STUCK:
      return new BamlPanic(
        "baml panic: the call can never finish: nothing can run, no timer is set and no I/O can complete",
      );
    case STATUS_HOST_THREW:
      return payload;
    case STATUS_DECODE:
      return new BamlError(`unexpected value from BAML: ${payload}`);
    default:
      return new BamlError(`a call ended with status ${status}`);
  }
}

/** A thrown value or a panic as today's bridge words it. */
function thrownError(kind, payload) {
  const { value, text } = payload;
  let { className } = payload;
  if (className === undefined && value !== null && typeof value !== "object" && typeof value !== "string") {
    className = undefined;
  } else if (className === undefined && typeof value === "string" && text) {
    // An instance of a class outside the interface arrives as its text,
    // which starts with its class name.
    const m = /^([A-Za-z_][\w]*(?:\.[A-Za-z_][\w]*)+)/.exec(text);
    if (m && value === text) className = m[1];
  }
  let message = "";
  if (value !== null && typeof value === "object" && typeof value.message === "string") {
    message = value.message;
  } else if (className !== undefined && typeof value === "string" && value === text) {
    const m = /message: "((?:[^"\\]|\\.)*)"/.exec(text);
    if (m) message = m[1];
  }
  const label = className || `baml.${kind}`;
  let formatted = `baml ${kind}: ${label}`;
  if (message) formatted += `: ${message}`;
  const detail = { value, bamlTrace: [], className };
  if (kind === "panic") return new BamlPanic(formatted, detail);
  switch (className) {
    case "baml.errors.InvalidArgument":
      return new BamlInvalidArgumentError(formatted, detail);
    case "baml.errors.GenericSdkError":
    case "baml.errors.CompilationError":
    case "baml.errors.AccessError":
      return new BamlClientError(formatted, detail);
    default:
      return new BamlError(formatted, detail);
  }
}

function startAsync(index, args) {
  let resolve;
  let reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  if (EAGER) {
    const [id, events] = addon.call(index, args);
    pending.set(id, { resolve, reject });
    deliver(events);
  } else {
    const id = addon.start(index, args);
    pending.set(id, { resolve, reject });
    scheduleTurn();
  }
  return promise;
}

function callSync(index, args) {
  const id = addon.start(index, args);
  const slot = { id, done: false, status: 0, payload: undefined };
  const outer = syncSlot;
  syncSlot = slot;
  try {
    deliver(addon.drive(id));
  } finally {
    syncSlot = outer;
  }
  if (slot.status === STATUS_RETURNED) return slot.payload;
  throw callError(slot.status, slot.payload);
}

// ─── defineFunction ───

/** Sentinel for "argument not supplied". */
export const UNSET = Symbol("baml.UNSET");

/** Positional arguments, then an options object, as today's bridge reads them. */
function buildArgs(args, requiredNames, optionalNames) {
  const positionalLimit = requiredNames.length;
  if (args.length > positionalLimit + 1) {
    throw new TypeError(
      `got ${args.length} positional arguments but only ${positionalLimit} positional ` +
        `parameter names (${JSON.stringify(requiredNames)})`,
    );
  }
  const built = Object.create(null);
  for (let i = 0; i < args.length && i < positionalLimit; i++) {
    if (args[i] === UNSET) continue;
    built[requiredNames[i]] = args[i];
  }
  if (args.length > positionalLimit) {
    const opts = args[positionalLimit];
    if (opts === undefined || opts === UNSET) return built;
    if (opts === null || Array.isArray(opts) || typeof opts !== "object") {
      throw new TypeError("optional arguments must be passed as an object");
    }
    for (const [key, value] of Object.entries(opts)) {
      if (key === "$baml" || key === "$types") {
        if (value !== undefined && value !== null && value !== UNSET) {
          throw new BamlError(`${key} is not supported by the native bridge yet`);
        }
        continue;
      }
      if (!optionalNames.includes(key)) {
        throw new TypeError(`unknown optional argument ${JSON.stringify(key)}`);
      }
      if (value === undefined || value === UNSET) continue;
      built[key] = value;
    }
  }
  return built;
}

/**
 * A free function: `defineFunction("user.ns.f", "async" | "sync", ["a"])`.
 * Its arguments are copied into BAML by their declared types; its result
 * comes back as today's bridge returns it.
 */
export function defineFunction(bamlFqn, mode, requiredParamNames, optionalParamNames, generics) {
  if (mode !== "sync" && mode !== "async") {
    throw new Error(`mode must be 'sync' or 'async', got ${JSON.stringify(mode)}`);
  }
  const requiredNames = [...requiredParamNames];
  const optionalNames = [...(optionalParamNames ?? [])];
  const exportName = bamlFqn.startsWith("user.") ? bamlFqn.slice(5) : bamlFqn;
  const generic = (generics?.typeParams?.length ?? 0) > 0;
  // Resolved at the first call: the export's number and, when the generated
  // names are its parameters in order, a direct path for positional calls.
  let target = null;
  const resolve = () => {
    ready();
    const found = exportsByName.get(exportName);
    if (found === undefined || generic) {
      throw new BamlError(`${bamlFqn} is not in the native library: it did not pack`);
    }
    const direct =
      optionalNames.length === 0 &&
      found.params.length === requiredNames.length &&
      found.params.every((name, i) => name === requiredNames[i]);
    target = { index: found.index, params: found.params, direct };
    return target;
  };
  const argsOf = (t, args) => {
    if (t.direct && args.length <= requiredNames.length && !args.includes(UNSET)) return args;
    const kwargs = buildArgs(args, requiredNames, optionalNames);
    return t.params.map((name) => kwargs[name]);
  };
  if (mode === "async") {
    return (...args) => {
      try {
        const t = target ?? resolve();
        if (!bound) ready();
        return startAsync(t.index, argsOf(t, args));
      } catch (err) {
        return Promise.reject(err instanceof BamlError || err instanceof TypeError ? err : wrapNativeError(err));
      }
    };
  }
  return (...args) => {
    let t;
    let built;
    try {
      t = target ?? resolve();
      if (!bound) ready();
      built = argsOf(t, args);
    } catch (err) {
      throw err instanceof BamlError || err instanceof TypeError ? err : wrapNativeError(err);
    }
    try {
      return callSync(t.index, built);
    } catch (err) {
      throw err instanceof Error && !("code" in err) ? err : wrapNativeError(err);
    }
  };
}

// ─── what the native bridge does not do yet: stubs that throw when used ───

function notSupported(what) {
  return new BamlError(`${what} is not supported by the native bridge yet`);
}

function stubClass(name) {
  const C = class {
    constructor() {
      throw notSupported(name);
    }
  };
  Object.defineProperty(C, "name", { value: name });
  return new Proxy(C, {
    get(target, key, receiver) {
      if (key in target || typeof key === "symbol") return Reflect.get(target, key, receiver);
      return () => {
        throw notSupported(`${name}.${String(key)}`);
      };
    },
  });
}

function stubFunction(name) {
  return () => {
    throw notSupported(name);
  };
}

export function defineInstanceFunction(bamlFqn) {
  return { bind: () => stubFunction(`instance method ${bamlFqn}`) };
}

export const BamlRuntime = stubClass("BamlRuntime");
export const BamlHandle = stubClass("BamlHandle");
export const BamlImage = stubClass("BamlImage");
export const BamlAudio = stubClass("BamlAudio");
export const BamlVideo = stubClass("BamlVideo");
export const BamlPdf = stubClass("BamlPdf");
export const BamlStream = stubClass("BamlStream");
export const BamlFunctionSpec = stubClass("BamlFunctionSpec");
export const BamlPrompt = stubClass("BamlPrompt");
export const BamlReference = stubClass("BamlReference");
export const BamlType = stubClass("BamlType");
export const Never = stubClass("Never");
export const FunctionResult = stubClass("FunctionResult");
export const _ExecutionContext = stubClass("ExecutionContext");
export class TraceUsageError extends Error {}
export const getRuntime = stubFunction("getRuntime");
export const getBridgeRuntimeVersion = stubFunction("getBridgeRuntimeVersion");
export const getToolchainVersion = stubFunction("getToolchainVersion");
export const getVersion = stubFunction("getVersion");
export const shutdownRuntime = () => {};
export const _seedGenericMediaHandle = stubFunction("_seedGenericMediaHandle");
export const encodeCallArgs = stubFunction("encodeCallArgs");
export const decodeCallResult = stubFunction("decodeCallResult");
export const lowerTypeToWireTy = stubFunction("lowerTypeToWireTy");
export const reflectType = stubFunction("reflectType");
export const initializeRuntime = stubFunction("initializeRuntime (a BAML source tree)");
export const initializeRuntimeFromBlob = stubFunction("initializeRuntimeFromBlob (bytecode)");
export const newFunctionCall = stubFunction("newFunctionCall");
export const cancelFunctionCall = stubFunction("cancelFunctionCall");
export const callFunctionSync = stubFunction("callFunctionSync");
export const callFunction = stubFunction("callFunction");
export const _currentExecutionContext = stubFunction("_currentExecutionContext");
export const _currentTraceContext = stubFunction("_currentTraceContext");
export const _currentTraceContextAsync = stubFunction("_currentTraceContextAsync");
export const _currentCancelToken = stubFunction("_currentCancelToken");
export const _withExecutionContext = stubFunction("_withExecutionContext");
export const _invoke = stubFunction("_invoke");
export const _invokeSync = stubFunction("_invokeSync");
export const _instrument = stubFunction("tracing (instrument)");
export const registerCapture = stubFunction("registerCapture");
export const captureFor = stubFunction("captureFor");
// ─── for tests (imported from "@boundaryml/baml-bridge/native", so that the
// package root exports the names today's bridge exports) ───

globalThis[Symbol.for("baml.native.testing")] = {
  handleCount: () => addon.handleCount(),
  collect: () => addon.collect(),
  pending: () => pending.size,
  /** Start export `index` with `args` as given, past the bindings' checks. */
  startRaw: (index, args) => addon.start(index, args),
  build: BUILD,
  eager: EAGER,
};
