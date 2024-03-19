// Import the RawWrapper class
import { RawWrapper } from './raw_wrapper';

// TypeScript doesn't have a direct equivalent of Python's filter_empty function,
// so we'll create a utility function to filter out null or undefined values from an array
function filterEmpty<T>(array: Array<T | undefined>): Array<T> {
    return array.filter((item): item is T => item !== undefined);
}

// TypeScript equivalent of the ListRawWrapper class
class ListRawWrapper<T extends RawWrapper> extends RawWrapper {
    private __val: Array<T>;

    constructor(val: Array<T>) {
        super();
        this.__val = val;
    }

    as_self(): any {
        return this.__val.map(item => item.as_self());
    }

    as_str(inner: boolean): string | undefined {
        return JSON.stringify(this.as_self(), null, inner ? 0 : 2);
    }

    as_smart_str(inner: boolean): string | undefined {
        if (this.__val.length === 1) {
            return this.__val[0].as_smart_str(inner);
        }
        return this.as_str(true);
    }

    as_int(): number | undefined {
        if (this.__val.length === 0) {
            return undefined;
        }
        for (const item of filterEmpty(this.__val.map(v => v.as_int()))) {
            return item;
        }
        return undefined;
    }

    as_float(): number | undefined {
        if (this.__val.length === 0) {
            return undefined;
        }
        for (const item of filterEmpty(this.__val.map(v => v.as_float()))) {
            return item;
        }
        return undefined;
    }

    as_bool(): boolean | undefined {
        if (this.__val.length === 0) {
            return undefined;
        }
        for (const item of filterEmpty(this.__val.map(v => v.as_bool()))) {
            return item;
        }
        return undefined;
    }

    as_list(): Array<T> {
        return this.__val;
    }

    as_dict(): Map<RawWrapper | undefined, RawWrapper> {
        return new Map([[undefined, this]]);
    }
}

export { ListRawWrapper };