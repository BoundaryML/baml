import { BaseDeserializer, CheckLutFn, Result } from "./base_deserializer";
import { Diagnostics } from "./diagnostics";
import { RawWrapper } from "./raw_wrapper/raw_wrapper";
import { JSONSchema7 } from "json-schema";

class EnumDeserializer<T extends Record<string, string>> extends BaseDeserializer<keyof T> {
    private aliases: Map<string, string> = new Map();

    private constructor(public readonly name: string, private readonly values: Map<string, keyof T>, aliases: Record<string, string>) {
        super(3);
        
        // Aliases are case-insensitive
        Object.entries(aliases).forEach(([k, v]) => {
            this.aliases.set(k.toLowerCase(), v.toLowerCase());
        });
    }

    static from_schema<T extends Record<string, string>>(schema: JSONSchema7, aliases: Record<string, string>): EnumDeserializer<T> {
        if (schema.enum === undefined) {
            throw new Error(`Schema must be an enum`);
        }

        const name = schema.title ?? "unnamed_enum";
        const values = schema.enum.map(x => {
            if (x === null) {
                throw new Error(`Null schema not supported for value ${x}`);
            }
            if (typeof x === "boolean") {
                throw new Error(`Boolean schema not supported for value ${x}`);
            }
            if (typeof x === "number") {
                throw new Error(`Number schema not supported for value ${x}`);
            }
            if (Array.isArray(x)) {
                throw new Error(`Array schema not supported for value ${x}`);
            }

            if (typeof x === "string") {
                return x as keyof T & string;
            }

            const value = x.const;
            if (value === undefined || typeof value !== "string") {
                throw new Error(`Invalid schema for value ${x}`);
            }
            return value as keyof T & string;
        });

        return new EnumDeserializer(name, new Map(values.map(v => [v.toLowerCase(), v])), aliases);
    }

    copy_with_aliases(aliases: Record<string, string>): EnumDeserializer<T> {
        this.aliases.forEach((v, k) => {
            if (!aliases[k]) {
                aliases[k] = v;
            }
        });
        return new EnumDeserializer(this.name, this.values, aliases);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<keyof T> {
        diagnostics.pushScope(this.name);
        const parsed = raw.as_smart_str(false);
        if (parsed === undefined) {
            diagnostics.pushUnknownError(`Expected string, got ${raw.as_self()}`);
            diagnostics.popScope(false);
            return Result.failed();
        }

        const value = parsed.toLowerCase();
        const valName = this.values.get(value);
        if (!valName) {
            diagnostics.pushUnknownError(`Unknown value: ${parsed}`);
            diagnostics.popScope(false);
            return Result.failed();
        }
        diagnostics.popScope(true);

        return Result.from_value(valName);
    }
}

export { EnumDeserializer };