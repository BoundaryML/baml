// Import the RawWrapper class
import { RawWrapper } from './raw_wrapper';

// Abstract class RawBaseWrapper, generic type T
class RawBaseWrapper<T extends boolean | number> extends RawWrapper {
    private __val: T;

    constructor(val: T) {
        super();
        this.__val = val;
    }

    as_self(): T | undefined {
        return this.__val;
    }

    as_str(inner: boolean): string | undefined {
        return String(this.__val);
    }

    as_smart_str(inner: boolean): string | undefined {
        return String(this.__val).trim();
    }

    as_int(): number | undefined {
        if (typeof this.__val === 'number') {
            // Return value as an integer and not a float
            return Math.floor(this.__val);
        }
        return this.__val ? 1 : 0;
    }

    as_float(): number | undefined {
        if (typeof this.__val === 'number') {
            return this.__val;
        }
        return this.__val ? 1.0 : 0.0;
    }
    as_bool(): boolean | undefined {
        if (typeof this.__val === 'boolean') {
            return this.__val;
        }
        return this.__val ? true : false;
    }

    as_list(): Iterable<RawWrapper> {
        return [this].values();
    }

    as_dict(): Iterable<[RawWrapper | undefined, RawWrapper]> {
        return new Map([[undefined, this]]).entries();
    }
}

// Class RawStringWrapper
class RawStringWrapper extends RawWrapper {
    
    as_int(): number | undefined {
        if (this.__as_inner) {
            return this.__as_inner.as_int();
        }
        return undefined;
    }
    as_float(): number | undefined {
        if (this.__as_inner) {
            return this.__as_inner.as_float();
        }
        return undefined;
    }
    as_bool(): boolean | undefined {
        if (this.__as_inner) {
            return this.__as_inner.as_bool();
        }
        return undefined;
    }
    as_list(): Iterable<RawWrapper> {
        if (this.__as_inner) {
            return this.__as_inner.as_list();
        }
        if (this.__as_list) {
            return this.__as_list.as_list();
        }
        if (this.__as_obj) {
            return [this.__as_obj].values();
        }
        return [this].values();
    }

    as_dict(): Iterable<[RawWrapper | undefined, RawWrapper]> {
        if (this.__as_inner) {
            return this.__as_inner.as_dict();
        }
        if (this.__as_obj) {
            return this.__as_obj.as_dict();
        }
        return new Map([[undefined, this]]).entries();
    }

    private __val: string;
    private __as_obj: RawWrapper | undefined;
    private __as_list: RawWrapper | undefined;
    private __as_inner: RawWrapper | undefined;

    constructor(
        val: string,
        as_obj: RawWrapper | undefined,
        as_list: RawWrapper | undefined,
        as_inner: RawWrapper | undefined
    ) {
        super();
        this.__val = val;
        this.__as_obj = as_obj;
        this.__as_list = as_list;
        this.__as_inner = as_inner;
    }

    as_str(inner: boolean): string | undefined {
        return this.__val;
    }

    as_smart_str(inner: boolean): string | undefined {
        if (inner && this.__as_inner) {
            return this.__as_inner.as_smart_str(true);
        }

        const new_str = this.__val.trim();
        // Remove leading and trailing quotes
        if (new_str.startsWith('"') && new_str.endsWith('"')) {
            return new_str.slice(1, -1);
        }
        if (new_str.startsWith("'") && new_str.endsWith("'")) {
            return new_str.slice(1, -1);
        }
        return new_str;
    }

    // Implement other abstract methods
    // ...

    as_self(): string {
        return this.as_str(false) ?? '';
    }

    toString(): string {
        return `RawStringWrapper\n---\n${this.__val}\n---`;
    }
}

// Class RawNoneWrapper
class RawNoneWrapper extends RawWrapper {
    as_int(): number | undefined {
        return undefined;
    }
    as_float(): number | undefined {
        return undefined;
    }
    as_bool(): boolean | undefined {
        return undefined;
    }
    as_list(): Iterable<RawWrapper> {
        return [];
    }
    as_dict(): Iterable<[RawWrapper | undefined, RawWrapper]> {
        return [];
    }

    constructor() {
        super();
    }

    as_self(): any {
        return null;
    }

    as_str(inner: boolean): string | undefined {
        return undefined;
    }

    as_smart_str(inner: boolean): string | undefined {
        return undefined;
    }

    toString(): string {
        return "RawNoneWrapper\n---\nNone\n---";
    }
}

export { RawBaseWrapper, RawStringWrapper, RawNoneWrapper };