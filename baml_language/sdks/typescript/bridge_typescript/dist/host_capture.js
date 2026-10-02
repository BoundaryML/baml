/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import * as util from 'node:util';
import { BamlAbortError } from './errors.js';
export function capture(value) {
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