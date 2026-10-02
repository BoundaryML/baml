import * as util from 'node:util';
import { BamlAbortError } from './errors.js';

type Observation = [string, unknown?];
export function capture(value: unknown): string {
    let remaining = 512;
    let bytes = 64 * 1024;
    const active = new Set<object>();
    const text = (value: string): boolean => {
        const size = Buffer.byteLength(value);
        if (size > bytes) return false;
        bytes -= size;
        return true;
    };
    const copy = (value: unknown, depth: number): Observation => {
        if (depth > 8) return ['depth'];
        if (remaining-- <= 0) return ['values'];
        if (value === null) return ['null'];
        if (typeof value === 'boolean') return ['bool', value];
        if (typeof value === 'number') return Number.isFinite(value) ? ['number', value] : ['unavailable'];
        if (typeof value === 'string') return text(value) ? ['string', value] : ['bytes'];
        if (typeof value !== 'object' || util.types.isProxy(value) || active.has(value)) return ['unavailable'];
        const prototype = Object.getPrototypeOf(value);
        if (prototype !== Object.prototype && prototype !== Array.prototype && prototype !== null) return ['unavailable'];
        active.add(value);
        try {
            if (Array.isArray(value)) {
                const length = Object.getOwnPropertyDescriptor(value, 'length')!.value as number;
                if (length > remaining) return ['values'];
                const values: Observation[] = [];
                for (let index = 0; index < length; index++) {
                    if (remaining <= 0) return ['values'];
                    const field = Object.getOwnPropertyDescriptor(value, String(index));
                    values.push(field && 'value' in field ? copy(field.value, depth + 1) : ['unavailable']);
                }
                return ['list', values];
            }
            const entries: Array<[string, Observation]> = [];
            let scanned = 0;
            for (const key in value) {
                if (scanned++ >= 512) return ['values'];
                const field = Object.getOwnPropertyDescriptor(value, key);
                if (!field) continue;
                if (remaining <= 0) return ['values'];
                if (!text(key)) return ['bytes'];
                entries.push([key, 'value' in field ? copy(field.value, depth + 1) : ['unavailable']]);
            }
            return ['map', entries];
        } finally { active.delete(value); }
    };
    // Only fresh copied tuples reach JSON.stringify, never application values.
    const copied = copy(value, 0);
    const protect = (value: unknown): void => {
        if (!Array.isArray(value)) return;
        Object.defineProperty(value, 'toJSON', { value: undefined });
        for (let index = 0; index < value.length; index++) protect(value[index]);
    };
    protect(copied);
    return JSON.stringify(copied);
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
