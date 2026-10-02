/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Host execution stays on the ordinary Node executor. The native guard closes
// only after synchronous exit or native Promise settlement.
import * as util from 'node:util';
import { createHash } from 'node:crypto';
import { BamlHandle, _beginHostInvocation, _validateHostOptions } from './native.js';
import { current, Invocation } from './invocation.js';
import { getTypeMap } from './typemap.js';
import { BamlAbortError } from './errors.js';
export class TraceUsageError extends TypeError {
}
const nativeThen = Promise.prototype.then;
const nativeSpecies = Object.getOwnPropertyDescriptor(Promise, Symbol.species)?.get;
const abortErrorPrototype = BamlAbortError.prototype;
function callerSite() {
    // Unlike Error.stack, this does not call a user Error.prepareStackTrace.
    const getSites = util.getCallSites;
    const site = getSites?.()[2];
    return { sourceFile: site?.scriptName ?? '<native>', line: site?.lineNumber ?? 0, column: site?.columnNumber ?? 0 };
}
let diagnostics = 0;
function diagnostic(message) {
    if (diagnostics++ < 8) {
        try {
            console.warn(message);
        }
        catch { /* Logging cannot replace host exit. */ }
    }
}
function capture(value) {
    let remaining = 512;
    let bytes = 64 * 1024;
    const active = new Set();
    const text = (value) => {
        const size = Buffer.byteLength(value);
        if (size > bytes)
            return false;
        bytes -= size;
        return true;
    };
    const copy = (value, depth) => {
        if (depth > 8)
            return ['depth'];
        if (remaining-- <= 0)
            return ['values'];
        if (value === null)
            return ['null'];
        if (typeof value === 'boolean')
            return ['bool', value];
        if (typeof value === 'number')
            return Number.isFinite(value) ? ['number', value] : ['unavailable'];
        if (typeof value === 'string')
            return text(value) ? ['string', value] : ['bytes'];
        if (typeof value !== 'object' || util.types.isProxy(value) || active.has(value))
            return ['unavailable'];
        const prototype = Object.getPrototypeOf(value);
        if (prototype !== Object.prototype && prototype !== Array.prototype && prototype !== null)
            return ['unavailable'];
        active.add(value);
        try {
            if (Array.isArray(value)) {
                const length = Object.getOwnPropertyDescriptor(value, 'length').value;
                if (length > remaining)
                    return ['values'];
                const values = [];
                for (let index = 0; index < length; index++) {
                    if (remaining <= 0)
                        return ['values'];
                    const field = Object.getOwnPropertyDescriptor(value, String(index));
                    values.push(field && 'value' in field ? copy(field.value, depth + 1) : ['unavailable']);
                }
                return ['list', values];
            }
            const entries = [];
            let scanned = 0;
            for (const key in value) {
                if (scanned++ >= 512)
                    return ['values'];
                const field = Object.getOwnPropertyDescriptor(value, key);
                if (!field)
                    continue;
                if (remaining <= 0)
                    return ['values'];
                if (!text(key))
                    return ['bytes'];
                entries.push([key, 'value' in field ? copy(field.value, depth + 1) : ['unavailable']]);
            }
            return ['map', entries];
        }
        finally {
            active.delete(value);
        }
    };
    // Only fresh copied tuples reach JSON.stringify, never application values.
    const copied = copy(value, 0);
    const protect = (value) => {
        if (!Array.isArray(value))
            return;
        Object.defineProperty(value, 'toJSON', { value: undefined });
        for (let index = 0; index < value.length; index++)
            protect(value[index]);
    };
    protect(copied);
    return JSON.stringify(copied);
}
function optionsHandle(options) {
    if (options === null || options === undefined)
        return undefined;
    const Options = getTypeMap().getClass('trace.Options');
    if (typeof options !== 'object' || util.types.isProxy(options) || !(options instanceof Options)) {
        throw new TraceUsageError('instrument accepts generated trace.Options or null');
    }
    const handle = Object.getOwnPropertyDescriptor(options, '_handle')?.value;
    if (!(handle instanceof BamlHandle))
        throw new TraceUsageError('invalid host trace options handle');
    return handle;
}
export function instrument(optionsOrBody, body, display) {
    const target = body ?? optionsOrBody;
    if (typeof target !== 'function' || util.types.isProxy(target)
        || util.types.isGeneratorFunction(target)) {
        throw new TraceUsageError('instrument supports ordinary functions and async functions');
    }
    if (display?.name !== undefined && (typeof display.name !== 'string' || display.name.length === 0)) {
        throw new TraceUsageError('instrument name must be a nonempty string');
    }
    const options = optionsHandle(body === undefined ? undefined : optionsOrBody);
    let requests;
    try {
        requests = _validateHostOptions(options);
    }
    catch (error) {
        throw new TraceUsageError('invalid host trace options', { cause: error });
    }
    const site = callerSite();
    const name = Object.getOwnPropertyDescriptor(target, 'name')?.value;
    // Stable source content plus wrapper source coordinate; never object IDs,
    // display labels, request metadata, or invocation IDs. Missing source on
    // older Node releases is represented explicitly, with a content fallback.
    const source = Function.prototype.toString.call(target);
    const key = createHash('sha256').update(source).digest('hex');
    const definition = {
        module: site.sourceFile, qualifiedName: `${key}:${site.column}`,
        sourceFile: site.sourceFile, definitionLine: site.line,
        wrapperLine: site.line, displayName: display?.name ?? name ?? '<anonymous>',
    };
    return function (...args) {
        let execution;
        let active;
        try {
            const entered = _beginHostInvocation(definition, current()?.state, options, callerSite(), requests[0] ? capture(args) : undefined);
            execution = entered[0];
            active = new Invocation(entered[1], entered[2]);
        }
        catch (error) {
            execution?.abandon();
            const code = typeof error === 'object' && error !== null && !util.types.isProxy(error)
                ? Object.getOwnPropertyDescriptor(error, 'code')?.value : undefined;
            if (code === 'InvalidArg') {
                throw new TraceUsageError('invalid host execution context', { cause: error });
            }
            diagnostic('host trace entry failed');
            return Reflect.apply(target, this, args);
        }
        const failureOutcome = (error) => {
            try {
                let value = error;
                for (let depth = 0; depth < 32 && typeof value === 'object' && value !== null; depth++) {
                    if (util.types.isProxy(value))
                        return 'error';
                    if (value === abortErrorPrototype)
                        return 'cancelled';
                    value = Object.getPrototypeOf(value);
                }
                return 'error';
            }
            catch {
                return 'error';
            }
        };
        const finish = (outcome, value) => {
            try {
                execution.finish(outcome, requests[outcome === 'ok' ? 1 : 2] ? capture(value) : undefined);
            }
            catch {
                diagnostic('host trace completion failed');
            }
        };
        return active.run(() => {
            let result;
            try {
                result = Reflect.apply(target, this, args);
            }
            catch (error) {
                finish(failureOutcome(error), error);
                throw error;
            }
            if (util.types.isPromise(result)) {
                // Promise subclasses and altered species may execute application
                // getters when observed. Preserve their behavior and report a
                // recording failure rather than substituting a result.
                if (Object.getPrototypeOf(result) !== Promise.prototype
                    || Object.getOwnPropertyDescriptor(result, 'constructor')
                    || Object.getOwnPropertyDescriptor(Promise.prototype, 'constructor')?.value !== Promise
                    || Object.getOwnPropertyDescriptor(Promise, Symbol.species)?.get !== nativeSpecies) {
                    execution.abandon();
                    diagnostic('host trace cannot observe a customized Promise');
                }
                else {
                    try {
                        // Return the observed Promise so an unhandled rejection
                        // remains unhandled. Returning the original while adding
                        // a rejection observer would silently consume it.
                        return nativeThen.call(result, (value) => { finish('ok', value); return value; }, (error) => { finish(failureOutcome(error), error); throw error; });
                    }
                    catch {
                        execution.abandon();
                        diagnostic('host trace Promise observation failed');
                    }
                }
            }
            else {
                finish('ok', result);
            }
            return result;
        });
    };
}
//# sourceMappingURL=instrumentation.js.map