// Host execution stays on the ordinary Node executor. The native guard closes
// only after synchronous exit or native Promise settlement.
import * as util from 'node:util';
import { createHash } from 'node:crypto';
import { BamlHandle, _defineHostMarker, _beginHostInvocation, _validateHostOptions, type HostCallSite } from './native.js';
import { current, Invocation } from './invocation.js';
import { getTypeMap } from './typemap.js';
import { capture, failureOutcome, diagnostic } from './host_capture.js';
import { registerHostMarker } from './host_marker.js';
import { consumeHostAdoption } from './platform.js';

export class TraceUsageError extends TypeError {}
type Body = (this: any, ...args: any[]) => any;
type Display = { readonly name?: string };
type Site = HostCallSite & { column: number };
const nativeThen = Promise.prototype.then;
const nativeSpecies = Object.getOwnPropertyDescriptor(Promise, Symbol.species)?.get;

function callerSite(): Site {
    // Unlike Error.stack, this does not call a user Error.prepareStackTrace.
    const getSites = (util as unknown as { getCallSites?: () => Array<{
        scriptName: string; lineNumber: number; columnNumber: number;
    }> }).getCallSites;
    const site = getSites?.()[2];
    return { sourceFile: site?.scriptName ?? '<native>', line: site?.lineNumber ?? 0, column: site?.columnNumber ?? 0 };
}

function optionsHandle(options: unknown): BamlHandle | undefined {
    if (options === null || options === undefined) return undefined;
    const Options = getTypeMap().getClass('trace.Options') as new (...args: any[]) => object;
    if (typeof options !== 'object' || util.types.isProxy(options) || !(options instanceof Options)) {
        throw new TraceUsageError('instrument accepts generated trace.Options or null');
    }
    const handle = Object.getOwnPropertyDescriptor(options, '_handle')?.value;
    if (!(handle instanceof BamlHandle)) throw new TraceUsageError('invalid host trace options handle');
    return handle;
}

export function instrument<F extends Body>(body: F): F;
export function instrument<F extends Body>(options: unknown, body: F, display?: Display): F;
export function instrument<F extends Body>(optionsOrBody: unknown, body?: F, display?: Display): F {
    const target = body ?? optionsOrBody;
    if (typeof target !== 'function' || util.types.isProxy(target)
        || util.types.isGeneratorFunction(target)) {
        throw new TraceUsageError('instrument supports ordinary functions and async functions');
    }
    if (display?.name !== undefined && (typeof display.name !== 'string' || display.name.length === 0)) {
        throw new TraceUsageError('instrument name must be a nonempty string');
    }
    const options = optionsHandle(body === undefined ? undefined : optionsOrBody);
    let requests: boolean[];
    try { requests = _validateHostOptions(options); }
    catch (error) { throw new TraceUsageError('invalid host trace options', { cause: error }); }
    const site = callerSite();
    const name = Object.getOwnPropertyDescriptor(target, 'name')?.value as string | undefined;
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
    const marker = { handle: _defineHostMarker(definition, options), identity: {} };
    const wrapped = function (this: unknown, ...args: unknown[]): unknown {
        if (consumeHostAdoption(marker.identity)) return Reflect.apply(target, this, args);
        let execution: ReturnType<typeof _beginHostInvocation>[0] | undefined;
        let active: Invocation;
        try {
            const entered = _beginHostInvocation(definition, current()?.state, options, callerSite(), requests[0] ? capture(args) : undefined);
            execution = entered[0];
            active = new Invocation(entered[1], entered[2]);
        } catch (error) {
            execution?.abandon();
            const code = typeof error === 'object' && error !== null && !util.types.isProxy(error)
                ? Object.getOwnPropertyDescriptor(error, 'code')?.value : undefined;
            if (code === 'InvalidArg') {
                throw new TraceUsageError('invalid host execution context', { cause: error });
            }
            diagnostic('host trace entry failed');
            return Reflect.apply(target, this, args);
        }
        const finish = (outcome: 'ok' | 'error' | 'cancelled', value: unknown): void => {
            try { execution!.finish(outcome, requests[outcome === 'ok' ? 1 : 2] ? capture(value) : undefined); }
            catch { diagnostic('host trace completion failed'); }
        };
        return active.run(() => {
            let result: unknown;
            try { result = Reflect.apply(target, this, args); }
            catch (error) { finish(failureOutcome(error), error); throw error; }
            if (util.types.isPromise(result)) {
                // Promise subclasses and altered species may execute application
                // getters when observed. Preserve their behavior and report a
                // recording failure rather than substituting a result.
                if (Object.getPrototypeOf(result) !== Promise.prototype
                    || Object.getOwnPropertyDescriptor(result, 'constructor')
                    || Object.getOwnPropertyDescriptor(Promise.prototype, 'constructor')?.value !== Promise
                    || Object.getOwnPropertyDescriptor(Promise, Symbol.species)?.get !== nativeSpecies) {
                    execution!.abandon();
                    diagnostic('host trace cannot observe a customized Promise');
                } else {
                    try {
                        // Return the observed Promise so an unhandled rejection
                        // remains unhandled. Returning the original while adding
                        // a rejection observer would silently consume it.
                        return nativeThen.call(result,
                            (value: unknown) => { finish('ok', value); return value; },
                            (error: unknown) => { finish(failureOutcome(error), error); throw error; },
                        );
                    } catch {
                        execution!.abandon();
                        diagnostic('host trace Promise observation failed');
                    }
                }
            } else { finish('ok', result); }
            return result;
        });
    } as F;
    registerHostMarker(wrapped, marker);
    return wrapped;
}
