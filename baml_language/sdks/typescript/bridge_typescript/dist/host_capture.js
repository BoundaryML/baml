/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import * as util from 'node:util';
import { BamlAbortError } from './errors.js';
import { getTypeMap } from './typemap.js';
const handlers = new Map();
let capturing = false;
const nativeDateTime = Date.prototype.getTime;
const nativeDateISO = Date.prototype.toISOString;
// Paired surrogates form one code point, so this matches only lone ones.
const loneSurrogate = /\p{Surrogate}/u;
export function registerCapture(valueType, handler) {
    if (typeof valueType !== 'function' || util.types.isProxy(valueType)
        || typeof handler !== 'function' || util.types.isProxy(handler)
        || util.types.isAsyncFunction(handler) || util.types.isGeneratorFunction(handler)) {
        throw new TypeError('registerCapture expects a constructor and a synchronous function');
    }
    handlers.set(valueType, handler);
    return handler;
}
export function captureFor(valueType) {
    return handler => registerCapture(valueType, handler);
}
function constructors(value) {
    const result = [];
    let prototype = Object.getPrototypeOf(value);
    for (let depth = 0; prototype !== null && depth < 32; depth++) {
        if (util.types.isProxy(prototype))
            return [];
        const ctor = Object.getOwnPropertyDescriptor(prototype, 'constructor')?.value;
        if (typeof ctor === 'function' && !util.types.isProxy(ctor))
            result.push(ctor);
        prototype = Object.getPrototypeOf(prototype);
    }
    return result;
}
function storedArray(value, maximum) {
    if (!Array.isArray(value) || util.types.isProxy(value))
        return undefined;
    const length = Object.getOwnPropertyDescriptor(value, 'length')?.value;
    if (typeof length !== 'number' || length > maximum)
        return undefined;
    const result = [];
    for (let index = 0; index < length; index++) {
        result.push(Object.getOwnPropertyDescriptor(value, String(index))?.value);
    }
    return result;
}
export function capture(value) {
    if (capturing)
        return '["unavailable"]';
    capturing = true;
    const registered = new Map(handlers);
    let remaining = 512;
    let bytes = 64 * 1024;
    const active = new Set();
    // Charges `value`'s UTF-8 size; undefined when it has no UTF-8 form, which
    // would make the whole observation undecodable by the native bridge.
    const text = (value) => {
        if (loneSurrogate.test(value))
            return undefined;
        const size = Buffer.byteLength(value);
        if (size > bytes)
            return false;
        bytes -= size;
        return true;
    };
    // Markers count too: the native decoder charges every node it reads.
    const opaque = () => (remaining-- <= 0 ? ['values'] : ['unavailable']);
    const copy = (value, depth) => {
        if (remaining-- <= 0)
            return ['values'];
        if (depth > 8)
            return ['depth'];
        if (value === null)
            return ['null'];
        if (typeof value === 'boolean')
            return ['bool', value];
        if (typeof value === 'number')
            return Number.isFinite(value) ? ['number', value] : ['unavailable'];
        if (typeof value === 'string') {
            const fits = text(value);
            return fits ? ['string', value] : fits === false ? ['bytes'] : ['unavailable'];
        }
        if (typeof value !== 'object' || util.types.isProxy(value) || active.has(value))
            return ['unavailable'];
        const prototype = Object.getPrototypeOf(value);
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
                    values.push(field && 'value' in field ? copy(field.value, depth + 1) : opaque());
                }
                return ['list', values];
            }
            const ctors = constructors(value);
            const native = ctors.map(ctor => getTypeMap().captureType(ctor)).find(name => name !== '');
            if (!native) {
                for (const ctor of prototype === Object.prototype || prototype === null ? [] : ctors) {
                    const handler = registered.get(ctor);
                    if (handler)
                        return copy(handler(value), depth + 1);
                }
                if (util.types.isDate(value)) {
                    return Number.isFinite(nativeDateTime.call(value)) ? copy(nativeDateISO.call(value), depth) : ['unavailable'];
                }
                if (util.types.isNativeError(value)) {
                    const projected = Object.create(null);
                    projected.type = 'Error';
                    let proto = prototype;
                    for (let step = 0; proto !== null && step < 32; step++) {
                        if (util.types.isProxy(proto))
                            break;
                        const name = Object.getOwnPropertyDescriptor(proto, 'name')?.value;
                        if (typeof name === 'string') {
                            projected.type = name;
                            break;
                        }
                        // `class ValidationError extends Error {}` has no own `name`;
                        // record its class name, as Python records `__qualname__`.
                        const ctor = Object.getOwnPropertyDescriptor(proto, 'constructor')?.value;
                        const ctorName = typeof ctor === 'function' && !util.types.isProxy(ctor)
                            ? Object.getOwnPropertyDescriptor(ctor, 'name')?.value : undefined;
                        if (typeof ctorName === 'string' && ctorName !== '') {
                            projected.type = ctorName;
                            break;
                        }
                        proto = Object.getPrototypeOf(proto);
                    }
                    const name = Object.getOwnPropertyDescriptor(value, 'name')?.value;
                    if (typeof name === 'string')
                        projected.type = name;
                    const message = Object.getOwnPropertyDescriptor(value, 'message');
                    projected.message = message && 'value' in message ? message.value : '';
                    const cause = Object.getOwnPropertyDescriptor(value, 'cause');
                    if (cause && 'value' in cause)
                        projected.cause = cause.value;
                    return copy(projected, depth);
                }
                if (prototype !== Object.prototype && prototype !== null)
                    return ['unavailable'];
            }
            const entries = [];
            let scanned = 0;
            for (const key in value) {
                if (scanned++ >= 512)
                    return ['values'];
                const field = Object.getOwnPropertyDescriptor(value, key);
                if (!field)
                    continue;
                if (native && key === '$types')
                    continue;
                if (remaining <= 0)
                    return ['values'];
                const fits = text(key);
                if (fits === undefined)
                    return ['unavailable'];
                if (!fits)
                    return ['bytes'];
                entries.push([key, 'value' in field ? copy(field.value, depth + 1) : opaque()]);
            }
            if (native) {
                if (!text(native))
                    return ['bytes'];
                const params = ctors.map(ctor => storedArray(Object.getOwnPropertyDescriptor(ctor, '$generic')?.value, remaining)).find(params => params !== undefined);
                const types = Object.getOwnPropertyDescriptor(value, '$types')?.value;
                const args = [];
                if (params) {
                    for (let index = 0; index < params.length; index++) {
                        const parameter = params[index];
                        if (typeof parameter !== 'string')
                            return ['unavailable'];
                        const token = typeof types === 'object' && types !== null && !util.types.isProxy(types)
                            ? Object.getOwnPropertyDescriptor(types, parameter)?.value : undefined;
                        args.push(typeObservation(token, depth + 1));
                    }
                }
                // The native decoder anchors the name to the engine's actual declaration.
                return ['class', native, entries, args];
            }
            return ['map', entries];
        }
        catch {
            diagnostic('host capture projection failed');
            return ['unavailable'];
        }
        finally {
            active.delete(value);
        }
    };
    const typeObservation = (token, depth) => {
        if (remaining-- <= 0 || depth > 8)
            return ['unknown'];
        if (typeof token === 'string' && ['int', 'float', 'string', 'bool', 'null'].includes(token))
            return [token];
        if (typeof token === 'function' && !util.types.isProxy(token)) {
            if (token === String)
                return ['string'];
            if (token === Boolean)
                return ['bool'];
            const name = getTypeMap().captureType(token);
            return name && text(name) ? ['class', name, []] : ['unknown'];
        }
        if (typeof token === 'object' && token !== null && !util.types.isProxy(token)
            && (Object.getPrototypeOf(token) === Object.prototype || Object.getPrototypeOf(token) === null)) {
            const field = (key) => Object.getOwnPropertyDescriptor(token, key)?.value;
            const list = field('list');
            if (list !== undefined)
                return ['list', typeObservation(list, depth + 1)];
            const map = storedArray(field('map'), 2);
            if (map?.length === 2)
                return ['map', typeObservation(map[0], depth + 1), typeObservation(map[1], depth + 1)];
            const optional = field('optional');
            if (optional !== undefined)
                return ['union', [typeObservation(optional, depth + 1), ['null']]];
            const union = storedArray(field('union'), remaining);
            if (union)
                return ['union', union.map(token => typeObservation(token, depth + 1))];
            const cls = field('class');
            if (typeof cls === 'function' && !util.types.isProxy(cls)) {
                const name = getTypeMap().captureType(cls);
                const args = storedArray(field('args'), remaining);
                if (name && text(name) && args) {
                    return ['class', name, args.map(token => typeObservation(token, depth + 1))];
                }
            }
            const name = getTypeMap().captureType(token);
            if (name && text(name))
                return ['enum', name];
        }
        return ['unknown'];
    };
    // Only fresh copied tuples reach JSON.stringify, never application values.
    try {
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
    finally {
        capturing = false;
    }
}
export function failureOutcome(error) {
    try {
        let value = error;
        for (let depth = 0; depth < 32 && typeof value === 'object' && value !== null; depth++) {
            if (util.types.isProxy(value))
                return 'error';
            if (value === BamlAbortError.prototype)
                return 'cancelled';
            value = Object.getPrototypeOf(value);
        }
    }
    catch { /* Classification cannot change the escaping error. */ }
    return 'error';
}
let diagnostics = 0;
export function diagnostic(message) {
    if (diagnostics++ < 8) {
        try {
            console.warn(message);
        }
        catch { /* Logging cannot replace host exit. */ }
    }
}
//# sourceMappingURL=host_capture.js.map