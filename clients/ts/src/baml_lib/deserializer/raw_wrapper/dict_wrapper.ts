// Import the RawWrapper class
import { RawWrapper } from './raw_wrapper';

// TypeScript doesn't have a direct equivalent of Python's filter_empty function,
// so we'll create a utility function to filter out null or undefined values from an array of tuples
function filterEmpty<T>(array: Array<[RawWrapper, T | undefined | undefined]>): Array<[RawWrapper, T]> {
    return array.filter((item): item is [RawWrapper, T] => item[1] !== null && item[1] !== undefined);
}

// TypeScript equivalent of the DictRawWrapper class
class DictRawWrapper extends RawWrapper {
    private __val: Map<RawWrapper, RawWrapper>;

    constructor(val: Map<RawWrapper, RawWrapper>) {
        super();
        this.__val = val;
    }

    as_self(): any {
        let result: {[key: string]: any} = {};
        this.__val.forEach((value, key) => {
            result[key.as_self()] = value.as_self();
        });
        return result;
    }

    as_str(inner: boolean): string | undefined {
        return JSON.stringify(this.as_self(), null, 2);
    }

    as_smart_str(inner: boolean): string | undefined {
        return this.as_str(true);
    }

    as_int(): number | undefined {
        if (this.__val.size === 1) {
            for (const [key, value] of filterEmpty(Array.from(this.__val.entries()).map(kv => [kv[0], kv[1].as_int()]))) {
                return value;
            }
        }
        return undefined;
    }

    as_float(): number | undefined {
        if (this.__val.size === 1) {
            for (const [key, value] of filterEmpty(Array.from(this.__val.entries()).map(kv => [kv[0], kv[1].as_float()]))) {
                return value;
            }
        }
        return undefined;
    }

    as_bool(): boolean | undefined {
        if (this.__val.size === 1) {
            for (const [key, value] of filterEmpty(Array.from(this.__val.entries()).map(kv => [kv[0], kv[1].as_bool()]))) {
                return value;
            }
        }
        return undefined;
    }

    as_list(): Iterable<RawWrapper> {
        return Array.from(this.__val.values());
    }

    as_dict(): Iterable<[RawWrapper | undefined, RawWrapper]> {
        return this.__val.entries();
    }
}

export { DictRawWrapper };