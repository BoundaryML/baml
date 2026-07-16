import initWasm, {
  callFunction as callWasmFunction,
  callFunctionSync as callWasmFunctionSync,
  cancelFunctionCall as cancelWasmFunctionCall,
  completeWebHostCall,
  configureWebSysops,
  mintWebHostValueKey,
  newFunctionCall as newWasmFunctionCall,
  registerWebHostCallable,
  registerWebHostValueReleaseCallback,
  releaseWebHostCallable,
  stageRuntimeBytecode,
} from "./wasm/bridge_web_core.js";
await initWasm();

interface WebFetchCall {
  method: string;
  url: string;
  headers: Array<[string, string]>;
  body: Uint8Array;
  timeoutNanos: bigint;
}

type WebFetchResult =
  | { kind: "ok"; statusCode: number; url: string; headers: Array<[string, string]>; body: Uint8Array }
  | { kind: "io"; message: string }
  | { kind: "timeout"; message: string };

type WebReadFileResult =
  | { kind: "ok"; bytes: Uint8Array }
  | { kind: "io"; message: string }
  | { kind: "unavailable"; message: string };

type ReadFileSync = (path: string) => Uint8Array;
type HostCallableDispatchFactory = (callable: (...args: unknown[]) => unknown) => (callId: number, payload: Uint8Array) => void;

let readFileSyncImpl: ReadFileSync | undefined;
let hostCallableDispatchFactory: HostCallableDispatchFactory | undefined;
let webSysopKeys: { fetch: bigint; readFileSync: bigint } | undefined;

export function installReadFileSync(impl: ReadFileSync): void {
  readFileSyncImpl = impl;
}

export function installHostCallableDispatchFactory(factory: HostCallableDispatchFactory): void {
  hostCallableDispatchFactory = factory;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function parseFetchCall(value: unknown): WebFetchCall {
  if (value === null || typeof value !== "object") throw new TypeError("fetch call must be an object");
  const call = value as Partial<WebFetchCall>;
  if (typeof call.method !== "string") throw new TypeError("fetch method must be a string");
  if (typeof call.url !== "string") throw new TypeError("fetch URL must be a string");
  if (!(call.body instanceof Uint8Array)) throw new TypeError("fetch body must be bytes");
  if (typeof call.timeoutNanos !== "bigint" || call.timeoutNanos < 0n) throw new TypeError("fetch timeoutNanos must be a non-negative bigint");
  if (!Array.isArray(call.headers) || !call.headers.every((pair) => Array.isArray(pair) && pair.length === 2 && typeof pair[0] === "string" && typeof pair[1] === "string")) {
    throw new TypeError("fetch headers must be string pairs");
  }
  return call as WebFetchCall;
}

async function webFetch(value: unknown): Promise<WebFetchResult> {
  let call: WebFetchCall;
  try {
    call = parseFetchCall(value);
  } catch (error) {
    return { kind: "io", message: errorMessage(error) };
  }

  const controller = new AbortController();
  let timedOut = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  if (call.timeoutNanos > 0n) {
    const timeoutMillis = (call.timeoutNanos + 999_999n) / 1_000_000n;
    const maximumTimerMillis = 2_147_483_647n;
    timer = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, Number(timeoutMillis > maximumTimerMillis ? maximumTimerMillis : timeoutMillis));
  }

  try {
    const method = call.method.toUpperCase();
    const requestBody = method === "GET" || method === "HEAD" || call.body.length === 0 ? undefined : new Uint8Array(call.body);
    const response = await globalThis.fetch(call.url, {
      method,
      headers: new Headers(call.headers),
      body: requestBody,
      signal: controller.signal,
    });
    const body = new Uint8Array(await response.arrayBuffer());
    return {
      kind: "ok",
      statusCode: response.status,
      url: response.url || call.url,
      headers: Array.from(response.headers.entries()),
      body,
    };
  } catch (error) {
    return timedOut
      ? { kind: "timeout", message: `HTTP request timed out: ${errorMessage(error)}` }
      : { kind: "io", message: `HTTP request failed: ${errorMessage(error)}` };
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

function webReadFileSync(path: unknown): WebReadFileResult {
  if (typeof path !== "string") return { kind: "io", message: "readFileSync path must be a string" };
  if (!readFileSyncImpl) return { kind: "unavailable", message: "fs.readFileSync is not available in this JavaScript runtime" };
  try {
    const value = readFileSyncImpl(path);
    const bytes = new Uint8Array(value.byteLength);
    bytes.set(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
    return { kind: "ok", bytes };
  } catch (error) {
    return { kind: "io", message: `readFileSync failed for ${path}: ${errorMessage(error)}` };
  }
}

function ensureWebSysopsConfigured(): void {
  if (!hostCallableDispatchFactory) throw new Error("Web HOST_VALUE_CALLABLE dispatch is not installed");
  if (!webSysopKeys) {
    webSysopKeys = {
      fetch: registerWebHostCallable(hostCallableDispatchFactory(webFetch)),
      readFileSync: registerWebHostCallable(hostCallableDispatchFactory(webReadFileSync)),
    };
  }
  configureWebSysops(webSysopKeys.fetch, webSysopKeys.readFileSync);
}

export interface HandleKey { low: number; high: number; }

function keyFromBigint(value: bigint): HandleKey {
  return { low: Number(BigInt.asIntN(32, value)), high: Number(BigInt.asIntN(32, value >> 32n)) };
}

export class BamlHandle {
  constructor(public readonly key: HandleKey, public readonly handleType: number) {}
  clone(): BamlHandle { return new BamlHandle(this.key, this.handleType); }
  _cloneKeyForWire(): HandleKey { return this.key; }
}

class BamlMedia {
  protected constructor(private readonly handle: BamlHandle) {}
  static fromUrl(_url: string, _mimeType?: string | null): BamlMedia { throw new Error("browser media construction is not implemented yet"); }
  static fromFile(_file: string, _mimeType?: string | null): BamlMedia { throw new Error("browser media construction is not implemented yet"); }
  static fromBase64(_base64: string, _mimeType?: string | null): BamlMedia { throw new Error("browser media construction is not implemented yet"); }
  static _fromHandle(handle: BamlHandle): BamlMedia { return new this(handle); }
  _toHandle(): BamlHandle { return this.handle.clone(); }
  url(): string | null { return null; }
  file(): string | null { return null; }
  base64(): string { throw new Error("browser media access is not implemented yet"); }
  mimeType(): string | null { return null; }
}

export class BamlImage extends BamlMedia {}
export class BamlAudio extends BamlMedia {}
export class BamlVideo extends BamlMedia {}
export class BamlPdf extends BamlMedia {}

export class BamlCallContext {
  private callIds = new Set<bigint>();
  private isAborted = false;
  abort(): void {
    this.isAborted = true;
    for (const callId of this.callIds) cancelWasmFunctionCall(callId);
  }
  get aborted(): boolean { return this.isAborted; }
  _attachCallId(callId: string): void {
    const id = BigInt(callId);
    this.callIds.add(id);
    if (this.isAborted) cancelWasmFunctionCall(id);
  }
  _detachCallId(callId: string): void { this.callIds.delete(BigInt(callId)); }
}

export class HostSpanManager {
  enter(_name: string, _args: unknown): void {}
  exitOk(): void {}
  exitError(_message: string): void {}
  upsertTags(_tags: Record<string, string>): void {}
  deepClone(): HostSpanManager { return new HostSpanManager(); }
  contextDepth(): number { return 0; }
}

export class Timing {}
export class Usage {}
export class LlmCall {}
export { LlmCall as LLMCall };
export class FunctionLog {}
export class Collector {
  constructor(_name?: string | null) {}
}

export class BamlRuntime {
  static initializeRuntimeFromBytecode(bytecode: Uint8Array): BamlRuntime {
    ensureWebSysopsConfigured();
    stageRuntimeBytecode(bytecode);
    runtime = new BamlRuntime();
    return runtime;
  }
  static initializeRuntime(_rootPath: string, _files: Record<string, string>): BamlRuntime {
    throw new Error("browser source initialization is not implemented; use bytecode");
  }
  callFunctionSync(functionName: string, encodedArgs: Uint8Array, _ctx?: HostSpanManager | null, _collectors?: Collector[] | null): Uint8Array {
    return callWasmFunctionSync(functionName, encodedArgs);
  }
  callFunction(functionName: string, encodedArgs: Uint8Array, _ctx?: HostSpanManager | null, _collectors?: Collector[] | null): Promise<Uint8Array> {
    return callWasmFunction(functionName, encodedArgs);
  }
}

let runtime: BamlRuntime | undefined;
let hostRelease: ((key: HandleKey) => void) | undefined;

export function getRuntime(): BamlRuntime {
  if (!runtime) throw new Error("BAML runtime has not been initialized");
  return runtime;
}
export function newFunctionCall(): bigint { return newWasmFunctionCall(); }
export function cancelFunctionCall(callId: bigint | string): boolean { return cancelWasmFunctionCall(BigInt(callId)); }
export function flushEvents(): void {}
export function getVersion(): string { return "0.0.0-web"; }
export function mintHostValueKey(): HandleKey { return keyFromBigint(mintWebHostValueKey()); }
export function registerHostValueReleaseCallback(callback: (key: HandleKey) => void): void {
  hostRelease = callback;
  registerWebHostValueReleaseCallback((key: bigint) => hostRelease?.(keyFromBigint(key)));
}
export function registerHostCallable(callback: (callId: number, args: Uint8Array) => void): HandleKey { return keyFromBigint(registerWebHostCallable(callback)); }
export function releaseHostCallable(key: HandleKey): void {
  const value = BigInt.asUintN(64, BigInt(key.high) << 32n | BigInt.asUintN(32, BigInt(key.low)));
  releaseWebHostCallable(value);
  hostRelease?.(key);
}
export function completeHostCall(callId: number, isError: number, content: Uint8Array): void { completeWebHostCall(callId, isError, content); }
export function _seedFunctionRefHandle(_globalIndex: number): [HandleKey, number] { throw new Error("test handle seeding is not implemented yet"); }
export function _seedGenericMediaHandle(): [HandleKey, number] { throw new Error("test handle seeding is not implemented yet"); }
