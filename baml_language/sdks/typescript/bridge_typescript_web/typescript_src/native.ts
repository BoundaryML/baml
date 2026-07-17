import initWasm, {
  callFunction as callWasmFunction,
  callFunctionSync as callWasmFunctionSync,
  cancelFunctionCall as cancelWasmFunctionCall,
  newFunctionCall as newWasmFunctionCall,
  stageRuntimeBytecode,
} from "./wasm/bridge_web_core.js";

await initWasm();

type HostCallableDispatchFactory = (callable: (...args: unknown[]) => unknown) => (callId: number, payload: Uint8Array) => void;

export function installHostCallableDispatchFactory(_factory: HostCallableDispatchFactory): void {}

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
let nextHostValueKey = 1n;

export function getRuntime(): BamlRuntime {
  if (!runtime) throw new Error("BAML runtime has not been initialized");
  return runtime;
}
export function newFunctionCall(): bigint { return newWasmFunctionCall(); }
export function cancelFunctionCall(callId: bigint | string): boolean { return cancelWasmFunctionCall(BigInt(callId)); }
export function flushEvents(): void {}
export function getVersion(): string { return "0.0.0-web"; }
export function mintHostValueKey(): HandleKey { return keyFromBigint(nextHostValueKey++); }
export function registerHostValueReleaseCallback(callback: (key: HandleKey) => void): void { hostRelease = callback; }
export function registerHostCallable(_callback: (callId: number, args: Uint8Array) => void): HandleKey { return mintHostValueKey(); }
export function releaseHostCallable(key: HandleKey): void { hostRelease?.(key); }
export function completeHostCall(_callId: number, _isError: number, _content: Uint8Array): void {}
export function _seedFunctionRefHandle(_globalIndex: number): [HandleKey, number] { throw new Error("test handle seeding is not implemented yet"); }
export function _seedGenericMediaHandle(): [HandleKey, number] { throw new Error("test handle seeding is not implemented yet"); }
