import * as util from 'node:util';
import { BamlAbortError } from './errors.js';
import { getTypeMap } from './typemap.js';

type Observation = [string, ...unknown[]];
type CaptureHandler<T = any> = (value: T) => unknown;
const handlers = new Map<Function, CaptureHandler>();
let capturing = false;
const nativeDateTime = Date.prototype.getTime;
const nativeDateISO = Date.prototype.toISOString;
const nativeStackGetter = Object.getOwnPropertyDescriptor(new Error(), 'stack')?.get;
const unavailable = Symbol('host capture unavailable');
const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const typedField = (name: string | symbol, value: object) => Object.getOwnPropertyDescriptor(typedArrayPrototype, name)!.get!.call(value);
const regexpSource = Object.getOwnPropertyDescriptor(RegExp.prototype, 'source')!.get!;
function dataField(value: object, key: string): PropertyDescriptor | undefined {
    let current: object | null = value;
    for (let depth = 0; current !== null && depth < 32; depth++) {
        if (util.types.isProxy(current)) return undefined;
        const field = Object.getOwnPropertyDescriptor(current, key);
        if (field) return field;
        current = Object.getPrototypeOf(current);
    }
    return undefined;
}

// Paired surrogates form one code point, so this matches only lone ones.
const loneSurrogate = /\p{Surrogate}/u;

export function registerCapture<T>(valueType: new (...args: any[]) => T, handler: CaptureHandler<T>): CaptureHandler<T> {
    if (typeof valueType !== 'function' || util.types.isProxy(valueType)
        || typeof handler !== 'function' || util.types.isProxy(handler)
        || util.types.isAsyncFunction(handler) || util.types.isGeneratorFunction(handler)) {
        throw new TypeError('registerCapture expects a constructor and a synchronous function');
    }
    handlers.set(valueType, handler);
    return handler;
}

export function captureFor<T>(valueType: new (...args: any[]) => T): (handler: CaptureHandler<T>) => CaptureHandler<T> {
    return handler => registerCapture(valueType, handler);
}

function constructors(value: object): Function[] {
    const result: Function[] = [];
    let prototype = Object.getPrototypeOf(value);
    for (let depth = 0; prototype !== null && depth < 32; depth++) {
        if (util.types.isProxy(prototype)) return [];
        const ctor = Object.getOwnPropertyDescriptor(prototype, 'constructor')?.value;
        if (typeof ctor === 'function' && !util.types.isProxy(ctor)) result.push(ctor);
        prototype = Object.getPrototypeOf(prototype);
    }
    return result;
}

function storedArray(value: unknown, maximum: number): unknown[] | undefined {
    if (!Array.isArray(value) || util.types.isProxy(value)) return undefined;
    const length = Object.getOwnPropertyDescriptor(value, 'length')?.value;
    if (typeof length !== 'number' || length > maximum) return undefined;
    const result: unknown[] = [];
    for (let index = 0; index < length; index++) {
        result.push(Object.getOwnPropertyDescriptor(value, String(index))?.value);
    }
    return result;
}

export function capture(value: unknown): string {
    if (capturing) return '["unavailable"]';
    capturing = true;
    const registered = new Map(handlers);
    let remaining = 512;
    let bytes = 64 * 1024;
    const active = new Set<object>();
    // Charges `value`'s UTF-8 size; undefined when it has no UTF-8 form, which
    // would make the whole observation undecodable by the native bridge.
    const text = (value: string): boolean | undefined => {
        if (loneSurrogate.test(value)) return undefined;
        const size = Buffer.byteLength(value);
        if (size > bytes) return false;
        bytes -= size;
        return true;
    };
    // Markers count too: the native decoder charges every node it reads.
    const opaque = (): Observation => (remaining-- <= 0 ? ['values'] : ['unavailable']);
    const copy = (value: unknown, depth: number): Observation => {
        if (remaining-- <= 0) return ['values'];
        if (depth > 8) return ['depth'];
        if (value === null) return ['null'];
        if (value === unavailable) return ['unavailable'];
        if (value === undefined) return copy({ $undefined: true }, depth);
        if (typeof value === 'bigint') return copy({ $bigint: BigInt.prototype.toString.call(value) }, depth);
        if (typeof value === 'symbol') {
            const description = Object.getOwnPropertyDescriptor(Symbol.prototype, 'description')!.get!.call(value);
            return copy({ $symbol: { description, key: Symbol.keyFor(value) } }, depth);
        }
        if (typeof value === 'boolean') return ['bool', value];
        if (typeof value === 'number') {
            if (Object.is(value, -0)) return copy({ $number: '-0' }, depth);
            return Number.isFinite(value) ? ['number', value] : copy({ $number: Number.isNaN(value) ? 'NaN' : value > 0 ? 'Infinity' : '-Infinity' }, depth);
        }
        if (typeof value === 'string') {
            const fits = text(value);
            return fits ? ['string', value] : fits === false ? ['bytes'] : ['unavailable'];
        }
        if (typeof value !== 'object' || util.types.isProxy(value) || active.has(value)) return ['unavailable'];
        const prototype = Object.getPrototypeOf(value);
        active.add(value);
        try {
            if (Array.isArray(value)) {
                const length = Object.getOwnPropertyDescriptor(value, 'length')!.value as number;
                if (length > remaining) return ['values'];
                const values: Observation[] = [];
                for (let index = 0; index < length; index++) {
                    if (remaining <= 0) return ['values'];
                    const field = Object.getOwnPropertyDescriptor(value, String(index));
                    values.push(!field ? copy({ $hole: true }, depth + 1) : 'value' in field ? copy(field.value, depth + 1) : opaque());
                }
                return ['list', values];
            }
            const ctors = constructors(value);
            const native = ctors.map(ctor => getTypeMap().captureType(ctor)).find(name => name !== '');
            if (!native) {
                for (const ctor of prototype === Object.prototype || prototype === null ? [] : ctors) {
                    const handler = registered.get(ctor);
                    if (handler) return copy(handler(value), depth + 1);
                }
                for (const [matches, unbox] of [
                    [util.types.isNumberObject, Number.prototype.valueOf],
                    [util.types.isBooleanObject, Boolean.prototype.valueOf],
                    [util.types.isStringObject, String.prototype.valueOf],
                    [util.types.isBigIntObject, BigInt.prototype.valueOf],
                    [util.types.isSymbolObject, Symbol.prototype.valueOf],
                ] as const) {
                    if (matches(value)) return copy(Reflect.apply(unbox, value, []), depth);
                }
                if (util.types.isDate(value)) {
                    return Number.isFinite(nativeDateTime.call(value)) ? copy(nativeDateISO.call(value), depth) : copy({ $date: 'invalid' }, depth);
                }
                if (util.types.isNativeError(value)) {
                    const fields = Object.create(null) as Record<string, unknown>;
                    const attributes = Object.create(null) as Record<string, unknown>;
                    const ctor = dataField(value, 'constructor')?.value;
                    fields.type = typeof ctor === 'function' && !util.types.isProxy(ctor)
                        ? Object.getOwnPropertyDescriptor(ctor, 'name')?.value : 'Error';
                    for (const key of ['name', 'message']) {
                        const field = dataField(value, key);
                        fields[key] = field && 'value' in field ? field.value : unavailable;
                    }
                    const stack = Object.getOwnPropertyDescriptor(value, 'stack');
                    fields.stack = stack && 'value' in stack ? stack.value : unavailable;
                    const prepare = dataField(Error, 'prepareStackTrace');
                    if (stack?.get && stack.get === nativeStackGetter
                        && (!prepare || ('value' in prepare && prepare.value === undefined))
                        && ['name', 'message'].every(key => {
                            const field = dataField(value, key);
                            return !field || ('value' in field && typeof field.value === 'string');
                        })) {
                        try { fields.stack = stack.get.call(value); } catch { /* Keep other fields. */ }
                    }
                    const cause = dataField(value, 'cause');
                    if (cause) fields.cause = 'value' in cause ? cause.value : unavailable;
                    let scanned = 0;
                    for (const key of Object.getOwnPropertyNames(value)) {
                        if (scanned++ >= 512) break;
                        if (['name', 'message', 'stack', 'cause'].includes(key)) continue;
                        const field = Object.getOwnPropertyDescriptor(value, key)!;
                        if (key === 'errors') fields.errors = 'value' in field ? field.value : unavailable;
                        else attributes[key] = 'value' in field ? field.value : unavailable;
                    }
                    fields.attributes = scanned > 512 ? unavailable : attributes;
                    return copy(fields, depth);
                }
                if (typeof DOMException !== 'undefined' && prototype === DOMException.prototype) {
                    const field = (key: string) => Object.getOwnPropertyDescriptor(DOMException.prototype, key)!.get!.call(value);
                    return copy({ type: 'DOMException', name: field('name'), message: field('message'), code: field('code') }, depth);
                }
                if (util.types.isMap(value)) {
                    if (Object.getOwnPropertyDescriptor(Map.prototype, 'size')!.get!.call(value) > remaining / 3) return ['values'];
                    const entries: unknown[][] = [];
                    for (const [key, item] of Map.prototype.entries.call(value)) {
                        entries.push([key, item]);
                    }
                    return copy({ $map: entries }, depth);
                }
                if (util.types.isSet(value)) {
                    if (Object.getOwnPropertyDescriptor(Set.prototype, 'size')!.get!.call(value) > remaining) return ['values'];
                    return copy({ $set: Array.from(Set.prototype.values.call(value)) }, depth);
                }
                if (util.types.isUint8Array(value)) {
                    const length = typedField('byteLength', value);
                    if (length * 2 > bytes) return ['bytes'];
                    const buffer = typedField('buffer', value);
                    const offset = typedField('byteOffset', value);
                    return copy({ encoding: 'hex', data: Buffer.from(buffer, offset, length).toString('hex') }, depth);
                }
                if (util.types.isTypedArray(value)) {
                    const length = typedField('length', value);
                    if (length > remaining) return ['values'];
                    const values = [];
                    for (let index = 0; index < length; index++) values.push(Object.getOwnPropertyDescriptor(value, String(index))!.value);
                    return copy({ type: typedField(Symbol.toStringTag, value), values }, depth);
                }
                if (util.types.isAnyArrayBuffer(value) || util.types.isDataView(value)) {
                    const view = util.types.isDataView(value);
                    const proto = view ? DataView.prototype : util.types.isSharedArrayBuffer(value) ? SharedArrayBuffer.prototype : ArrayBuffer.prototype;
                    const length = Object.getOwnPropertyDescriptor(proto, 'byteLength')!.get!.call(value);
                    if (length * 2 > bytes) return ['bytes'];
                    const buffer = view ? Object.getOwnPropertyDescriptor(DataView.prototype, 'buffer')!.get!.call(value) : value;
                    const offset = view ? Object.getOwnPropertyDescriptor(DataView.prototype, 'byteOffset')!.get!.call(value) : 0;
                    return copy({ encoding: 'hex', data: Buffer.from(buffer, offset, length).toString('hex') }, depth);
                }
                if (util.types.isRegExp(value)) {
                    const flags = [['hasIndices', 'd'], ['global', 'g'], ['ignoreCase', 'i'], ['multiline', 'm'], ['dotAll', 's'], ['unicode', 'u'], ['unicodeSets', 'v'], ['sticky', 'y']]
                        .filter(([name]) => Object.getOwnPropertyDescriptor(RegExp.prototype, name)?.get?.call(value)).map(([, flag]) => flag).join('');
                    return copy({ $regexp: { source: regexpSource.call(value), flags, lastIndex: Object.getOwnPropertyDescriptor(value, 'lastIndex')?.value } }, depth);
                }
                if (prototype === URL.prototype) {
                    const url = new URL(URL.prototype.toString.call(value));
                    url.username = ''; url.password = ''; url.search = ''; url.hash = '';
                    return copy(URL.prototype.toString.call(url), depth);
                }
                if ((typeof Request !== 'undefined' && prototype === Request.prototype)
                    || (typeof Response !== 'undefined' && prototype === Response.prototype)) {
                    const request = prototype === Request.prototype;
                    const rawURL = Object.getOwnPropertyDescriptor(prototype, 'url')!.get!.call(value);
                    const url = rawURL ? new URL(rawURL) : null;
                    if (url) { url.username = ''; url.password = ''; url.search = ''; url.hash = ''; }
                    return copy(request
                        ? { method: Object.getOwnPropertyDescriptor(prototype, 'method')!.get!.call(value), url: url?.toString() ?? null }
                        : { status_code: Object.getOwnPropertyDescriptor(prototype, 'status')!.get!.call(value), url: url?.toString() ?? null }, depth);
                }
                if (prototype !== Object.prototype && prototype !== null) return ['unavailable'];
            }
            const entries: Array<[string, Observation]> = [];
            let scanned = 0;
            for (const key in value) {
                if (scanned++ >= 512) return ['values'];
                const field = Object.getOwnPropertyDescriptor(value, key);
                if (!field) continue;
                if (native && key === '$types') continue;
                if (remaining <= 0) return ['values'];
                const fits = text(key);
                if (fits === undefined) return ['unavailable'];
                if (!fits) return ['bytes'];
                entries.push([key, 'value' in field ? copy(field.value, depth + 1) : opaque()]);
            }
            if (native) {
                if (!text(native)) return ['bytes'];
                const params = ctors.map(ctor => storedArray(Object.getOwnPropertyDescriptor(ctor, '$generic')?.value, remaining)).find(params => params !== undefined);
                const types = Object.getOwnPropertyDescriptor(value, '$types')?.value;
                const args: unknown[] = [];
                if (params) {
                    for (let index = 0; index < params.length; index++) {
                        const parameter = params[index];
                        if (typeof parameter !== 'string') return ['unavailable'];
                        const token = typeof types === 'object' && types !== null && !util.types.isProxy(types)
                            ? Object.getOwnPropertyDescriptor(types, parameter)?.value : undefined;
                        args.push(typeObservation(token, depth + 1));
                    }
                }
                // The native decoder anchors the name to the engine's actual declaration.
                return ['class', native, entries, args];
            }
            return ['map', entries];
        } catch { diagnostic('host capture projection failed'); return ['unavailable']; }
        finally { active.delete(value); }
    };
    const typeObservation = (token: unknown, depth: number): unknown[] => {
        if (remaining-- <= 0 || depth > 8) return ['unknown'];
        if (typeof token === 'string' && ['int', 'float', 'string', 'bool', 'null'].includes(token)) return [token];
        if (typeof token === 'function' && !util.types.isProxy(token)) {
            if (token === String) return ['string'];
            if (token === Boolean) return ['bool'];
            const name = getTypeMap().captureType(token);
            return name && text(name) ? ['class', name, []] : ['unknown'];
        }
        if (typeof token === 'object' && token !== null && !util.types.isProxy(token)
            && (Object.getPrototypeOf(token) === Object.prototype || Object.getPrototypeOf(token) === null)) {
            const field = (key: string) => Object.getOwnPropertyDescriptor(token, key)?.value;
            const list = field('list');
            if (list !== undefined) return ['list', typeObservation(list, depth + 1)];
            const map = storedArray(field('map'), 2);
            if (map?.length === 2) return ['map', typeObservation(map[0], depth + 1), typeObservation(map[1], depth + 1)];
            const optional = field('optional');
            if (optional !== undefined) return ['union', [typeObservation(optional, depth + 1), ['null']]];
            const union = storedArray(field('union'), remaining);
            if (union) return ['union', union.map(token => typeObservation(token, depth + 1))];
            const cls = field('class');
            if (typeof cls === 'function' && !util.types.isProxy(cls)) {
                const name = getTypeMap().captureType(cls);
                const args = storedArray(field('args'), remaining);
                if (name && text(name) && args) {
                    return ['class', name, args.map(token => typeObservation(token, depth + 1))];
                }
            }
            const name = getTypeMap().captureType(token);
            if (name && text(name)) return ['enum', name];
        }
        return ['unknown'];
    };
    // Only fresh copied tuples reach JSON.stringify, never application values.
    try {
        const copied = copy(value, 0);
        const protect = (value: unknown): void => {
            if (!Array.isArray(value)) return;
            Object.defineProperty(value, 'toJSON', { value: undefined });
            for (let index = 0; index < value.length; index++) protect(value[index]);
        };
        protect(copied);
        return JSON.stringify(copied);
    } finally { capturing = false; }
}

export function failureOutcome(error: unknown): 'error' | 'cancelled' {
    try {
        let value = error;
        for (let depth = 0; depth < 32 && typeof value === 'object' && value !== null; depth++) {
            if (util.types.isProxy(value)) return 'error';
            if (value === BamlAbortError.prototype) return 'cancelled';
            value = Object.getPrototypeOf(value);
        }
    } catch { /* Classification cannot change the escaping error. */ }
    return 'error';
}

let diagnostics = 0;
export function diagnostic(message: string): void {
    if (diagnostics++ < 8) {
        try { console.warn(message); } catch { /* Logging cannot replace host exit. */ }
    }
}
