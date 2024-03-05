import { BaseDeserializer, CheckLutFn, Result } from "./base_deserializer";
import { Diagnostics } from "./diagnostics";
import { RawWrapper } from "./raw_wrapper/raw_wrapper";


class ListDeserializer<T> extends BaseDeserializer<T[]> {
    constructor(private item: BaseDeserializer<T>) {
        super(1);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T[]> {
        const result: T[] = [];
        for (const item of raw.as_list()) {
            const d = new Diagnostics('');
            const coerced = this.item.coerce(item, d, fromLut);
            if (coerced.has_value) {
                result.push(coerced.as_value);
            } else {
                // TODO: merge diagnostics
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
        const d = new Diagnostics('');
        const res = this.item.coerce(raw, d, fromLut);
        if (res.has_value) {
            return res;
        } else {
            // TODO: merge diagnostics
            return Result.from_value(null);
        }
    }
}

class UnionDeserializer<T> extends BaseDeserializer<T> {
    constructor(private items: BaseDeserializer<any>[]) {
        super(1);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T> {
        const d = new Diagnostics('');
        for (const item of this.items) {
            const coerced = item.coerce(raw, d, fromLut);
            if (coerced.has_value) {
                return coerced;
            }
            // TODO: merge diagnostics
        }
        return Result.failed();
    }
}

export { ListDeserializer, OptionalDeserializer, UnionDeserializer }