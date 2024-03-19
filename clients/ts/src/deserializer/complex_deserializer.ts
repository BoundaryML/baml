import { BaseDeserializer, CheckLutFn, Result } from "./base_deserializer";
import { Diagnostics } from "./diagnostics";
import { RawWrapper } from "./raw_wrapper/raw_wrapper";


class ListDeserializer<T> extends BaseDeserializer<T[]> {
    constructor(private item: BaseDeserializer<T>) {
        super(1);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T[]> {
        const result: T[] = [];
        let idx = 0;
        for (const item of raw.as_list()) {
            diagnostics.pushScope(`${idx}`);
            const coerced = this.item.coerce(item, diagnostics, fromLut);
            diagnostics.popScope(true);
            if (coerced.has_value) {
                result.push(coerced.as_value);
            }
        }
        return Result.from_value(result);
    }
}

class OptionalDeserializer<T> extends BaseDeserializer<T | null> {
    constructor(private item: BaseDeserializer<T>) {
        super(1);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T | null> {
        diagnostics.pushScope('[optional]');
        const res = this.item.coerce(raw, diagnostics, fromLut);
        diagnostics.popScope(true);
        if (res.has_value) {
            return res;
        } else {
            return Result.from_value(null);
        }
    }
}

class UnionDeserializer<T> extends BaseDeserializer<T> {
    constructor(private items: BaseDeserializer<any>[]) {
        super(1);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T> {
        for (const item of this.items) {
            diagnostics.pushScope('[union]');
            const coerced = item.coerce(raw, diagnostics, fromLut);
            diagnostics.popScope(true);
            if (coerced.has_value) {
                return coerced;
            }
        }
        return Result.failed();
    }
}

export { ListDeserializer, OptionalDeserializer, UnionDeserializer }