// index.ts — mirrors bridge_python/python_src/baml_py/__init__.py

import {
    BamlRuntime,
    AbortController,
    BamlHandle,
    HostSpanManager,
    Collector as NativeCollector,
    FunctionLog as NativeFunctionLog,
    Timing,
    Usage,
    LLMCall,
    flushEvents,
} from './native';
import { encodeCallArgs, decodeCallResult } from './proto';

export { BamlRuntime, AbortController, BamlHandle, HostSpanManager, getVersion, flushEvents } from './native';
export { Timing, Usage, LLMCall } from './native';
export { encodeCallArgs, decodeCallResult } from './proto';
export { CtxManager } from './ctx_manager';
import { wrapNativeError } from './errors';
export {
    BamlError,
    BamlInvalidArgumentError,
    BamlClientError,
    BamlCancelledError,
    wrapNativeError,
} from './errors';

export class FunctionResult {
    private _value: unknown;

    constructor(value: unknown) {
        this._value = value;
    }

    result(): unknown {
        return this._value;
    }

    toString(): string {
        return `FunctionResult(${JSON.stringify(this._value)})`;
    }
}

export class FunctionLog {
    private _inner: NativeFunctionLog;
    constructor(inner: NativeFunctionLog) { this._inner = inner; }
    get id(): string { return this._inner.id; }
    get functionName(): string { return this._inner.functionName; }
    get timing(): Timing { return this._inner.timing; }
    get usage(): Usage { return this._inner.usage; }
    get calls(): LLMCall[] { return this._inner.calls; }
    get tags(): Record<string, string> { return this._inner.tags; }
    // FIXME: Returns null for both "no serialized result" (bytes == null) and a legitimate
    // BAML null result (decodeCallResult returns null). Legacy engine/ had no result getter
    // on FunctionLog at all. bridge_python has the same ambiguity (None for both cases).
    // Leaving as-is for parity with bridge_python; narrow edge case in practice.
    get result(): unknown {
        const bytes = this._inner.result;
        if (bytes == null) return null;
        return decodeCallResult(bytes);
    }
}

export class Collector {
    private _inner: NativeCollector;
    constructor(name?: string) { this._inner = new NativeCollector(name ?? null); }
    get name(): string { return this._inner.name; }
    get logs(): FunctionLog[] {
        return this._inner.logs.map((l: NativeFunctionLog) => new FunctionLog(l));
    }
    get last(): FunctionLog | null {
        const l = this._inner.last;
        return l ? new FunctionLog(l) : null;
    }
    get usage(): Usage { return this._inner.usage; }
    clear(): number { return this._inner.clear(); }
    id(functionLogId: string): FunctionLog | null {
        const l = this._inner.id(functionLogId);
        return l ? new FunctionLog(l) : null;
    }
    /** Internal: get native collector for passing to Rust */
    _native(): NativeCollector { return this._inner; }
}

export function callFunctionSync(
    rt: BamlRuntime,
    functionName: string,
    kwargs: Record<string, unknown>,
    ctx?: HostSpanManager,
    collectors?: Collector[],
    abortController?: AbortController,
): FunctionResult {
    const argsProto = encodeCallArgs(kwargs);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    try {
        const resultBytes = rt.callFunctionSync(functionName, argsProto, ctx ?? null, nativeCollectors, abortController ?? null);
        return new FunctionResult(decodeCallResult(resultBytes));
    } catch (err) {
        throw wrapNativeError(err);
    }
}

export async function callFunction(
    rt: BamlRuntime,
    functionName: string,
    kwargs: Record<string, unknown>,
    ctx?: HostSpanManager,
    collectors?: Collector[],
    abortController?: AbortController,
): Promise<FunctionResult> {
    const argsProto = encodeCallArgs(kwargs);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    try {
        const resultBytes = await rt.callFunction(functionName, argsProto, ctx ?? null, nativeCollectors, abortController ?? null);
        return new FunctionResult(decodeCallResult(resultBytes));
    } catch (err) {
        throw wrapNativeError(err);
    }
}

// Register flush on process exit (once to prevent duplicate handlers on module reload)
process.once('exit', () => {
    try { flushEvents(); } catch (_) { /* ignore */ }
});
