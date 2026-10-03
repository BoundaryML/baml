import initWasm, {
  callFunction as callWasmFunction,
  callFunctionSync as callWasmFunctionSync,
  cancelFunctionCall as cancelWasmFunctionCall,
  cloneHandle as cloneWasmHandle,
  completeWebHostCall,
  configureWebSysops,
  getBridgeRuntimeVersion as getWasmBridgeRuntimeVersion,
  getToolchainVersion as getWasmToolchainVersion,
  getVersion as getWasmVersion,
  mediaBase64 as mediaWasmBase64,
  mediaFile as mediaWasmFile,
  mediaFromBase64 as mediaWasmFromBase64,
  mediaFromFile as mediaWasmFromFile,
  mediaFromUrl as mediaWasmFromUrl,
  mediaMimeType as mediaWasmMimeType,
  mediaUrl as mediaWasmUrl,
  mintWebHostValueKey,
  newFunctionCall as newWasmFunctionCall,
  releaseFunctionCall as releaseWasmFunctionCall,
  invocationClockNs as invocationWasmClockNs,
  registerWebHostCancelCallback,
  traceSelection as wasmTraceSelection,
  invocationContext as wasmInvocationContext,
  cloneOutboundValue as wasmCloneOutboundValue,
  watchInvocationCancellation as wasmWatchInvocationCancellation,
  isInvocationCancelled as wasmIsInvocationCancelled,
  registerWebHostCallable,
  registerWebHostValueReleaseCallback,
  releaseHandle as releaseWasmHandle,
  releaseWebHostCallable,
  seedFunctionRefHandle as seedWasmFunctionRefHandle,
  seedGenericMediaHandle as seedWasmGenericMediaHandle,
  stageRuntimeBytecode,
  stageRuntimeSources,
} from "./wasm/bridge_web_core.js";
import { baml_bridge } from "./shared/proto/baml_cffi.js";
import { BamlClientError, wrapNativeError } from "./shared/errors.js";
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

function keyToBigint(key: HandleKey): bigint {
  if (!Number.isInteger(key.low) || !Number.isInteger(key.high)) {
    throw new TypeError("BamlHandle key halves must be 32-bit integers");
  }
  return (BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0);
}

function keyFromBigint(value: bigint): HandleKey {
  const normalized = BigInt.asUintN(64, value);
  return {
    low: Number(BigInt.asIntN(32, normalized)),
    high: Number(BigInt.asIntN(32, normalized >> 32n)),
  };
}

// Keep in sync with baml_handle.proto and baml_outbound.proto.
const HANDLE_FUNCTION_REF = 5;
const HANDLE_MEDIA_IMAGE = 6;
const HANDLE_MEDIA_AUDIO = 7;
const HANDLE_MEDIA_VIDEO = 8;
const HANDLE_MEDIA_PDF = 9;
const HANDLE_MEDIA_GENERIC = 10;
const HANDLE_HOST_VALUE_CALLABLE = 15;
const HANDLE_HOST_VALUE_OPAQUE = 16;
const MEDIA_IMAGE = 1;
const MEDIA_AUDIO = 2;
const MEDIA_PDF = 3;
const MEDIA_VIDEO = 4;

function isHostValueHandleType(handleType: number): boolean {
  return handleType === HANDLE_HOST_VALUE_CALLABLE || handleType === HANDLE_HOST_VALUE_OPAQUE;
}

const ordinaryHandleFinalizer = new FinalizationRegistry<bigint>((key) => {
  try {
    releaseWasmHandle(key);
  } catch {
    // Finalizers may run during isolate teardown. Eventual cleanup is best-effort.
  }
});

/** Release one owned wire reference that never reached a host wrapper. */
export function _releaseWireHandle(key: HandleKey): void {
  releaseWasmHandle(keyToBigint(key));
}

export class BamlHandle {
  private readonly rawKey: bigint;
  private readonly finalizerToken: object | undefined;
  private released = false;

  constructor(key: HandleKey, public readonly handleType: number) {
    this.rawKey = keyToBigint(key);
    if (isHostValueHandleType(handleType)) {
      this.finalizerToken = undefined;
    } else {
      this.finalizerToken = {};
      ordinaryHandleFinalizer.register(this, this.rawKey, this.finalizerToken);
    }
  }

  get key(): HandleKey {
    this.assertLive();
    return keyFromBigint(this.rawKey);
  }

  private assertLive(): void {
    if (this.released) throw new Error("BamlHandle has been released");
  }

  _keyForBridge(): bigint {
    this.assertLive();
    return this.rawKey;
  }

  clone(): BamlHandle {
    this.assertLive();
    if (isHostValueHandleType(this.handleType)) return new BamlHandle(this.key, this.handleType);
    return new BamlHandle(keyFromBigint(cloneWasmHandle(this.rawKey)), this.handleType);
  }

  _cloneKeyForWire(): HandleKey {
    this.assertLive();
    if (isHostValueHandleType(this.handleType)) return keyFromBigint(this.rawKey);
    return keyFromBigint(cloneWasmHandle(this.rawKey));
  }

  _releaseForTest(): boolean {
    if (isHostValueHandleType(this.handleType) || this.released) return false;
    this.released = true;
    if (this.finalizerToken) ordinaryHandleFinalizer.unregister(this.finalizerToken);
    return releaseWasmHandle(this.rawKey);
  }
}

abstract class BamlMedia {
  private readonly finalizerToken = {};
  private released = false;

  protected constructor(private readonly rawKey: bigint, private readonly handleType: number) {
    ordinaryHandleFinalizer.register(this, rawKey, this.finalizerToken);
  }

  protected static cloneKeyFromHandle(handle: BamlHandle, expectedHandleType: number, className: string): bigint {
    if (handle.handleType !== expectedHandleType) {
      throw new TypeError(`${className} requires handle type ${expectedHandleType}, got ${handle.handleType}`);
    }
    return cloneWasmHandle(handle._keyForBridge());
  }

  private assertLive(): void {
    if (this.released) throw new Error(`${this.constructor.name} has been released`);
  }

  _toHandle(): BamlHandle {
    this.assertLive();
    return new BamlHandle(keyFromBigint(cloneWasmHandle(this.rawKey)), this.handleType);
  }

  url(): string | null {
    this.assertLive();
    return mediaWasmUrl(this.rawKey, this.handleType) ?? null;
  }

  file(): string | null {
    this.assertLive();
    return mediaWasmFile(this.rawKey, this.handleType) ?? null;
  }

  base64(): string {
    this.assertLive();
    return mediaWasmBase64(this.rawKey, this.handleType);
  }

  mimeType(): string | null {
    this.assertLive();
    return mediaWasmMimeType(this.rawKey, this.handleType) ?? null;
  }

  _releaseForTest(): boolean {
    if (this.released) return false;
    this.released = true;
    ordinaryHandleFinalizer.unregister(this.finalizerToken);
    return releaseWasmHandle(this.rawKey);
  }
}

export class BamlImage extends BamlMedia {
  private constructor(rawKey: bigint) { super(rawKey, HANDLE_MEDIA_IMAGE); }
  static fromUrl(url: string, mimeType?: string | null): BamlImage { return new BamlImage(mediaWasmFromUrl(MEDIA_IMAGE, url, mimeType ?? undefined)); }
  static fromFile(file: string, mimeType?: string | null): BamlImage { return new BamlImage(mediaWasmFromFile(MEDIA_IMAGE, file, mimeType ?? undefined)); }
  static fromBase64(base64: string, mimeType?: string | null): BamlImage { return new BamlImage(mediaWasmFromBase64(MEDIA_IMAGE, base64, mimeType ?? undefined)); }
  static _fromHandle(handle: BamlHandle): BamlImage { return new BamlImage(this.cloneKeyFromHandle(handle, HANDLE_MEDIA_IMAGE, "BamlImage")); }
}

export class BamlAudio extends BamlMedia {
  private constructor(rawKey: bigint) { super(rawKey, HANDLE_MEDIA_AUDIO); }
  static fromUrl(url: string, mimeType?: string | null): BamlAudio { return new BamlAudio(mediaWasmFromUrl(MEDIA_AUDIO, url, mimeType ?? undefined)); }
  static fromFile(file: string, mimeType?: string | null): BamlAudio { return new BamlAudio(mediaWasmFromFile(MEDIA_AUDIO, file, mimeType ?? undefined)); }
  static fromBase64(base64: string, mimeType?: string | null): BamlAudio { return new BamlAudio(mediaWasmFromBase64(MEDIA_AUDIO, base64, mimeType ?? undefined)); }
  static _fromHandle(handle: BamlHandle): BamlAudio { return new BamlAudio(this.cloneKeyFromHandle(handle, HANDLE_MEDIA_AUDIO, "BamlAudio")); }
}

export class BamlVideo extends BamlMedia {
  private constructor(rawKey: bigint) { super(rawKey, HANDLE_MEDIA_VIDEO); }
  static fromUrl(url: string, mimeType?: string | null): BamlVideo { return new BamlVideo(mediaWasmFromUrl(MEDIA_VIDEO, url, mimeType ?? undefined)); }
  static fromFile(file: string, mimeType?: string | null): BamlVideo { return new BamlVideo(mediaWasmFromFile(MEDIA_VIDEO, file, mimeType ?? undefined)); }
  static fromBase64(base64: string, mimeType?: string | null): BamlVideo { return new BamlVideo(mediaWasmFromBase64(MEDIA_VIDEO, base64, mimeType ?? undefined)); }
  static _fromHandle(handle: BamlHandle): BamlVideo { return new BamlVideo(this.cloneKeyFromHandle(handle, HANDLE_MEDIA_VIDEO, "BamlVideo")); }
}

export class BamlPdf extends BamlMedia {
  private constructor(rawKey: bigint) { super(rawKey, HANDLE_MEDIA_PDF); }
  static fromUrl(url: string, mimeType?: string | null): BamlPdf { return new BamlPdf(mediaWasmFromUrl(MEDIA_PDF, url, mimeType ?? undefined)); }
  static fromFile(file: string, mimeType?: string | null): BamlPdf { return new BamlPdf(mediaWasmFromFile(MEDIA_PDF, file, mimeType ?? undefined)); }
  static fromBase64(base64: string, mimeType?: string | null): BamlPdf { return new BamlPdf(mediaWasmFromBase64(MEDIA_PDF, base64, mimeType ?? undefined)); }
  static _fromHandle(handle: BamlHandle): BamlPdf { return new BamlPdf(this.cloneKeyFromHandle(handle, HANDLE_MEDIA_PDF, "BamlPdf")); }
}

const MAX_UINT64 = (1n << 64n) - 1n;
const DECIMAL_UINT64 = /^\+?[0-9]+$/;
let callCancellationObserver: ((callId: bigint) => void) | undefined;

function parseCallId(callId: string): bigint {
  if (typeof callId !== "string" || !DECIMAL_UINT64.test(callId)) {
    throw new TypeError("callId must be a decimal uint64 string");
  }
  const parsed = BigInt(callId);
  if (parsed > MAX_UINT64) throw new TypeError("callId must be a decimal uint64 string");
  return parsed;
}

function cancelCallId(callId: bigint): boolean {
  callCancellationObserver?.(callId);
  return cancelWasmFunctionCall(callId);
}

export function _setCallCancellationObserverForTest(observer: ((callId: bigint) => void) | undefined): void {
  callCancellationObserver = observer;
}

export class BamlRuntime {
  static stageRuntimeFromBlob(bytecode: string | Uint8Array, embeddedBamlToml?: string): BamlRuntime {
    ensureWebSysopsConfigured();
    try {
      stageRuntimeBytecode(bytecode, embeddedBamlToml);
    } catch (error) {
      throw wrapNativeError(error);
    }
    runtime = new BamlRuntime();
    return runtime;
  }
  static stageRuntime(rootPath: string, files: Record<string, string>): BamlRuntime {
    ensureWebSysopsConfigured();
    try {
      stageRuntimeSources(rootPath, files);
    } catch (error) {
      throw wrapNativeError(error);
    }
    runtime = new BamlRuntime();
    return runtime;
  }
  callFunctionSync(encodedArgs: Uint8Array): Uint8Array {
    try {
      return callWasmFunctionSync(encodedArgs);
    } catch (error) {
      throw wrapNativeError(error);
    }
  }
  async callFunction(encodedArgs: Uint8Array): Promise<Uint8Array> {
    try {
      return await callWasmFunction(encodedArgs);
    } catch (error) {
      throw wrapNativeError(error);
    }
  }
}

let runtime: BamlRuntime | undefined;
let hostRelease: ((key: HandleKey) => void) | undefined;

export function getOrInitRuntime(): BamlRuntime {
  if (!runtime) throw new BamlClientError("BAML runtime has not been initialized");
  return runtime;
}
export function newFunctionCall(): bigint { return newWasmFunctionCall(); }
export function releaseFunctionCall(callId: string): boolean { return releaseWasmFunctionCall(parseCallId(callId)); }
export function invocationClockNs(callId: string): string { return invocationWasmClockNs(parseCallId(callId)).toString(); }
export interface WebHostExecution { state: BamlHandle; cancel: Uint8Array; cancelled: boolean; }
const hostExecutions = new Map<number, WebHostExecution>();
registerWebHostCancelCallback((id: number) => { const execution = hostExecutions.get(id); if (execution) execution.cancelled = true; });
export function _hostInvocationFrame(execution: object): [BamlHandle, Uint8Array] {
    const frame = execution as WebHostExecution;
    return [frame.state.clone(), wasmCloneOutboundValue(frame.cancel)];
}
export function _traceSelection(handle: BamlHandle, callId: string): [Uint8Array, BamlHandle | undefined] {
    const [wire, owner] = wasmTraceSelection(parseCallId(callId), keyToBigint(handle.key)) as [Uint8Array, bigint | null];
    return [wire, owner == null ? undefined : new BamlHandle(keyFromBigint(owner), 20)];
}
export function _invocationContext(handle?: BamlHandle): Uint8Array { return wasmInvocationContext(handle ? keyToBigint(handle.key) : 0n); }
export function _watchInvocationCancellation(handle: BamlHandle, callback: () => void): object { return wasmWatchInvocationCancellation(keyToBigint(handle.key), callback); }
export function _isInvocationCancelled(handle: BamlHandle): boolean { return wasmIsInvocationCancelled(keyToBigint(handle.key)); }
export function _startHostCallExecution(execution: object): string | null { return (execution as WebHostExecution).cancelled ? null : ""; }
export function _finishHostCallExecution(_execution: object): void { }
function releaseControlValue(value: baml_bridge.cffi.v1.IBamlOutboundValue): void {
    if (value.handleValue && !isHostValueHandleType(value.handleValue.handleType ?? 0)) releaseWasmHandle(BigInt(value.handleValue.key!.toString()));
    for (const item of value.listValue?.items ?? []) releaseControlValue(item);
    for (const item of value.mapValue?.entries ?? value.classValue?.fields ?? []) if (item.value) releaseControlValue(item.value);
    if (value.unionVariantValue?.value) releaseControlValue(value.unionVariantValue.value);
}
export function _discardHostCallArgs(bytes: Uint8Array): void {
  const call = baml_bridge.cffi.v1.BamlToHostCall.decode(bytes);
  for (const arg of call.args) if (arg.value) releaseControlValue(arg.value);
}
export function cancelFunctionCall(callId: bigint | string): boolean {
  const parsed = typeof callId === "bigint" ? callId : parseCallId(callId);
  if (parsed < 0n || parsed > MAX_UINT64) {
    throw new TypeError("callId must be a decimal uint64 string");
  }
  return cancelCallId(parsed);
}
export function getVersion(): string { return getWasmVersion(); }
export function getToolchainVersion(): string { return getWasmToolchainVersion(); }
export function getBridgeRuntimeVersion(): string { return getWasmBridgeRuntimeVersion(); }
export function mintHostValueKey(): HandleKey { return keyFromBigint(mintWebHostValueKey()); }
export function registerHostValueReleaseCallback(callback: (key: HandleKey) => void): void {
  hostRelease = callback;
  registerWebHostValueReleaseCallback((key: bigint) => hostRelease?.(keyFromBigint(key)));
}
export function registerHostCallable(callback: (callId: number, args: Uint8Array, execution?: object) => void, marker?: BamlHandle): HandleKey {
    if (marker) throw new TypeError("host instrumentation is not supported in Web");
    return keyFromBigint(registerWebHostCallable((bytes: Uint8Array) => {
        const wire = baml_bridge.cffi.v1.HostInvocation.decode(bytes);
        const execution = { state: new BamlHandle(keyFromBigint(BigInt(wire.effectiveState.toString())), 19), cancel: baml_bridge.cffi.v1.BamlOutboundValue.encode(wire.cancel!).finish(), cancelled: false };
        hostExecutions.set(wire.callbackId, execution);
        // Controls stay owned until completeHostCall reports actual exit.
        hostControls.set(wire.callbackId, wire.cancel ?? undefined);
        callback(wire.callbackId, wire.applicationArgs, execution);
    }));
}
const hostControls = new Map<number, baml_bridge.cffi.v1.IBamlOutboundValue | undefined>();
export function releaseHostCallable(key: HandleKey): void {
  const value = BigInt.asUintN(64, BigInt(key.high) << 32n | BigInt.asUintN(32, BigInt(key.low)));
  releaseWebHostCallable(value);
  hostRelease?.(key);
}
export function completeHostCall(callId: number, isError: number, content: Uint8Array): void {
    try { completeWebHostCall(callId, isError, content); }
    finally {
        const controls = hostControls.get(callId);
        hostControls.delete(callId);
        if (controls) releaseControlValue(controls);
        hostExecutions.delete(callId);
    }
}
export function _seedFunctionRefHandle(globalIndex: number): [HandleKey, number] {
  return [keyFromBigint(seedWasmFunctionRefHandle(globalIndex)), HANDLE_FUNCTION_REF];
}
export function _seedGenericMediaHandle(): [HandleKey, number] {
  return [keyFromBigint(seedWasmGenericMediaHandle()), HANDLE_MEDIA_GENERIC];
}
