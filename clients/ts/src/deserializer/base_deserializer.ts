import { JSONSchema7 } from "json-schema";
import { Diagnostics } from "./diagnostics";
import { RawWrapper } from "./raw_wrapper/raw_wrapper";

class Result<T> {
    has_value: boolean;
    value?: T;

    private constructor(has_value: boolean, value?: T) {
        this.has_value = has_value;
        this.value = value;
    }

    static from_value<T>(value: T): Result<T> {
        return new Result(true, value);
    }

    static failed<T>(): Result<T> {
        return new Result<T>(false, undefined);
    }

    get as_value(): T {
        if (!this.has_value) {
            throw new Error("Result does not have a value");
        }
        return this.value as T;
    }
}

type CheckLutFn<T> = (def: JSONSchema7) => BaseDeserializer<T>;

abstract class BaseDeserializer<T> {
    private _rank: number;

    constructor(rank: number) {
        this._rank = rank;
    }

    get rank() {
        return this._rank;
    }
    abstract coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T>;
}

export { BaseDeserializer, Result, CheckLutFn };