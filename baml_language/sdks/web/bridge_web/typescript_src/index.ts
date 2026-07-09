import initWasm, {
  callFunction as callCffiFunction,
  newFunctionCall as newCffiFunctionCall,
  stageRuntimeBytecode,
} from "./wasm/bridge_web_core.js";

await initWasm();

export type BamlPrimitiveToken = "int" | "float" | "string" | "bool" | "null" | "uint8array";
export type BamlType = BamlPrimitiveToken | BamlClassCtor | readonly BamlType[] | { [key: string]: BamlType };
export type BamlClassCtor = abstract new (...args: never[]) => unknown;

export interface BamlErrorDetail { value?: unknown; bamlTrace?: string[]; className?: string; }
export class BamlError extends Error {
  readonly value: unknown;
  readonly bamlTrace: string[];
  readonly className: string | undefined;
  constructor(message: string, detail?: BamlErrorDetail) {
    super(message);
    this.name = "BamlError";
    this.value = detail?.value;
    this.bamlTrace = detail?.bamlTrace ? [...detail.bamlTrace] : [];
    this.className = detail?.className;
  }
}

export class BamlInvalidArgumentError extends BamlError {}
export class BamlClientError extends BamlError {}
export class BamlCancelledError extends BamlError {}
export class BamlAbortError extends Error {
  constructor(message: string, public readonly reason?: unknown) { super(message); this.name = "AbortError"; }
}
export class BamlPanic extends BamlError {}

export class BamlHandle {
  constructor(public readonly key = 0, public readonly handleType = 0) {}
  clone(): BamlHandle { return new BamlHandle(this.key, this.handleType); }
  _cloneKeyForWire(): number { return this.key; }
}

class BamlMedia {
  protected constructor(private readonly kind: "url" | "file" | "base64", private readonly value: string, private readonly mime: string | null) {}
  static fromUrl(url: string, mimeType?: string | null): BamlMedia { return new this("url", url, mimeType ?? null); }
  static fromFile(file: string, mimeType?: string | null): BamlMedia { return new this("file", file, mimeType ?? null); }
  static fromBase64(base64: string, mimeType?: string | null): BamlMedia { return new this("base64", base64, mimeType ?? null); }
  url(): string | null { return this.kind === "url" ? this.value : null; }
  file(): string | null { return this.kind === "file" ? this.value : null; }
  base64(): string { return this.kind === "base64" ? this.value : ""; }
  mimeType(): string | null { return this.mime; }
  _toHandle(): BamlHandle { return new BamlHandle(); }
}

export class BamlImage extends BamlMedia {}
export class BamlAudio extends BamlMedia {}
export class BamlVideo extends BamlMedia {}
export class BamlPdf extends BamlMedia {}

export class BamlStream<TPartial = unknown, TFinal = unknown> implements AsyncIterable<TPartial | TFinal> {
  next(): TPartial { return invoke("baml.llm.Stream.next", [this]) as TPartial; }
  async nextAsync(): Promise<TPartial> { return this.next(); }
  final(): TFinal { return invoke("baml.llm.Stream.final", [this]) as TFinal; }
  async finalAsync(): Promise<TFinal> { return this.final(); }
  async *[Symbol.asyncIterator](): AsyncIterator<TPartial | TFinal> {
    throw new BamlError("bridge_cffi WASM streaming is not implemented yet");
  }
}

export type LazyEntry = () => unknown;

export class BamlTypeMap {
  private readonly classes = new Map<string, LazyEntry>();
  private readonly enums = new Map<string, LazyEntry>();
  private readonly aliases = new Map<string, LazyEntry>();

  static fromLazyEntries(args: { classes: Record<string, LazyEntry>; enums: Record<string, LazyEntry>; typeAliases: Record<string, LazyEntry> }): BamlTypeMap {
    const map = new BamlTypeMap();
    for (const [name, entry] of Object.entries(args.classes)) map.classes.set(name, entry);
    for (const [name, entry] of Object.entries(args.enums)) map.enums.set(name, entry);
    for (const [name, entry] of Object.entries(args.typeAliases)) map.aliases.set(name, entry);
    return map;
  }

  getClass(name: string): unknown { return this.classes.get(name)?.(); }
  getEnum(name: string): unknown { return this.enums.get(name)?.(); }
  getTypeAlias(name: string): unknown { return this.aliases.get(name)?.(); }
  jsTypeToBamlType(): string { return ""; }
}

let typeMap = new BamlTypeMap();
export function setTypeMap(map: BamlTypeMap): void { typeMap = map; }
export function getTypeMap(): BamlTypeMap { return typeMap; }

let initialized = false;
export function initializeRuntimeFromBytecode(bytecode: Uint8Array): void {
  stageRuntimeBytecode(bytecode);
  initialized = true;
}

export function initializeRuntime(_srcDir: string, _files: Record<string, string>): void {
  throw new BamlError("bridge_cffi WASM source initialization is not implemented yet");
}

function invoke(functionName: string, _args: unknown[]): Uint8Array {
  if (!initialized) throw new BamlError("BAML runtime has not been initialized");
  try {
    return callCffiFunction(functionName, new Uint8Array());
  } catch (error) {
    throw new BamlError(String(error));
  }
}

export class BamlRuntime {
  callFunctionSync(functionName: string, _args: Uint8Array, _ctx?: HostSpanManager | null, _collectors?: Collector[] | null): Uint8Array {
    return invoke(functionName, []);
  }
  async callFunction(functionName: string, _args: Uint8Array, _ctx?: HostSpanManager | null, _collectors?: Collector[] | null): Promise<Uint8Array> {
    return invoke(functionName, []);
  }
}

const runtime = new BamlRuntime();
export function getRuntime(): BamlRuntime {
  if (!initialized) throw new BamlError("BAML runtime has not been initialized");
  return runtime;
}

export function callFunctionSync(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], _callCtx?: BamlCallContext): FunctionResult {
  return new FunctionResult(rt.callFunctionSync(functionName, new TextEncoder().encode(JSON.stringify(kwargs)), ctx, collectors));
}

export async function callFunction(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], _callCtx?: BamlCallContext): Promise<FunctionResult> {
  return new FunctionResult(await rt.callFunction(functionName, new TextEncoder().encode(JSON.stringify(kwargs)), ctx, collectors));
}

export type Mode = "sync" | "async";
export interface GenericParams { typeParams?: readonly string[]; classTypeParams?: readonly string[]; }
export const UNSET: unique symbol = Symbol("baml.UNSET");

export function defineFunction(functionName: string, mode: Mode, _required: readonly string[], _optional?: readonly string[], _generics?: GenericParams): (...args: unknown[]) => unknown {
  if (mode === "sync") return (...args: unknown[]) => invoke(functionName, args);
  return async (...args: unknown[]) => invoke(functionName, args);
}

export function defineInstanceFunction(functionName: string, mode: Mode, required: readonly string[], optional?: readonly string[], generics?: GenericParams): { bind(self: unknown): (...args: unknown[]) => unknown } {
  return { bind: (self: unknown) => defineFunction(functionName, mode, required, optional, generics).bind(undefined, self) };
}

export class BamlCallContext {
  private callIds = new Set<string>();
  abort(): void { this.callIds.clear(); }
  get aborted(): boolean { return this.callIds.size === 0; }
  _attachCallId(id: string): void { this.callIds.add(id); }
  _detachCallId(id: string): void { this.callIds.delete(id); }
}

export function newFunctionCall(): bigint { return newCffiFunctionCall(); }
export function cancelFunctionCall(_callId: bigint): boolean { return false; }
export function flushEvents(): void {}
export function getVersion(): string { return "0.0.0-web-cffi-scaffold"; }

export const Never = Symbol("baml.Never");
export function lowerTypeToWireTy(value: BamlType): BamlType { return value; }

export class HostSpanManager {}
export class CtxManager {}
export class Timing {}
export class Usage {}
export class LLMCall {}
export class FunctionLog {}
export class Collector {}
export class FunctionResult { constructor(private readonly value: unknown) {} result(): unknown { return this.value; } }
