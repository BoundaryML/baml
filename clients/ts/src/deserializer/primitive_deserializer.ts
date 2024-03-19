import { BaseDeserializer, CheckLutFn, Result } from './base_deserializer';
import { Diagnostics } from './diagnostics';
import { RawWrapper } from './raw_wrapper/raw_wrapper';

type AsTypeFunction<T> = (raw: RawWrapper) => T | undefined;

class PrimitiveDeserializer<T extends string | number | boolean> extends BaseDeserializer<T> {
    private asType: AsTypeFunction<T>;
    private errorMessage: string;

    constructor(asType: AsTypeFunction<T>, errorMessage: string, rank: number) {
        super(rank);
        this.asType = asType;
        this.errorMessage = errorMessage;
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<T>): Result<T> {
        const parsed = this.asType(raw);
        if (parsed === undefined) {
            diagnostics.pushUnknownError(this.errorMessage);
            return Result.failed();
        }
        return Result.from_value(parsed);
    }
}

class NoneDeserializer extends BaseDeserializer<null> {
    constructor() {
        super(0);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<null>): Result<null> {
        return Result.from_value(null);
    }
}

export { NoneDeserializer, PrimitiveDeserializer };