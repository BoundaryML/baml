import { BamlCallContext, BamlRuntime, Collector, HostSpanManager, cancelFunctionCall as nativeCancelFunctionCall, getRuntime, installHostCallableDispatchFactory, newFunctionCall as nativeNewFunctionCall } from "./native.js";
import { decodeCallResult, encodeCallArgs, makeHostCallableDispatch } from "./shared/proto.js";

installHostCallableDispatchFactory(makeHostCallableDispatch);

export { BamlAudio, BamlCallContext, BamlHandle, BamlImage, BamlPdf, BamlRuntime, BamlVideo, Collector, FunctionLog, HostSpanManager, LLMCall, Timing, Usage, flushEvents, getRuntime, getVersion, newFunctionCall } from "./native.js";
export { BamlStream } from "./shared/stream.js";
export { BamlTypeMap, getTypeMap, setTypeMap } from "./shared/typemap.js";
export { defineFunction, defineInstanceFunction, UNSET } from "./shared/define_function.js";
export type { GenericParams } from "./shared/define_function.js";
export { Never, lowerTypeToWireTy } from "./shared/wire_ty.js";
export type { BamlClassCtor, BamlPrimitiveToken, BamlType } from "./shared/wire_ty.js";
export { BamlAbortError, BamlCancelledError, BamlClientError, BamlError, BamlInvalidArgumentError, BamlPanic } from "./shared/errors.js";
export { decodeCallResult, encodeCallArgs } from "./shared/proto.js";

export function initializeRuntimeFromBytecode(bytecode: Uint8Array): void {
  BamlRuntime.initializeRuntimeFromBytecode(bytecode);
}

export function initializeRuntime(srcDir: string, files: Record<string, string>): void {
  BamlRuntime.initializeRuntime(srcDir, files);
}

export class FunctionResult {
  constructor(private readonly value: unknown) {}
  result(): unknown { return this.value; }
  toString(): string { return `FunctionResult(${JSON.stringify(this.value)})`; }
}

export function callFunctionSync(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], callCtx?: BamlCallContext): FunctionResult {
  const callId = nativeNewFunctionCall();
  const args = encodeCallArgs(kwargs, { syncMode: true, callId });
  callCtx?._attachCallId(callId.toString());
  try {
    return new FunctionResult(decodeCallResult(rt.callFunctionSync(functionName, args, ctx ?? null, collectors ?? null)));
  } finally {
    callCtx?._detachCallId(callId.toString());
  }
}

export async function callFunction(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], callCtx?: BamlCallContext): Promise<FunctionResult> {
  const callId = nativeNewFunctionCall();
  const args = encodeCallArgs(kwargs, { callId });
  callCtx?._attachCallId(callId.toString());
  try {
    return new FunctionResult(decodeCallResult(await rt.callFunction(functionName, args, ctx ?? null, collectors ?? null)));
  } finally {
    callCtx?._detachCallId(callId.toString());
  }
}

export class CtxManager {
  private readonly context = new HostSpanManager();
  get(): HostSpanManager { return this.context; }
  allowReset(): boolean { return true; }
  reset(): void {}
  cloneContext(): HostSpanManager { return this.context.deepClone(); }
  upsertTags(tags: Record<string, string>): void { this.context.upsertTags(tags); }
}

export function cancelFunctionCall(callId: bigint): boolean {
  return nativeCancelFunctionCall(callId);
}
