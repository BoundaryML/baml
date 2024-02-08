import { BaseDeserializer, CheckLutFn, Result } from "./base_deserializer";
import { Diagnostics } from "./diagnostics";
import { RawWrapper } from "./raw_wrapper/raw_wrapper";
import { JSONSchema7 } from "json-schema";

class ObjectDeserializer<T extends Record<string, any>> extends BaseDeserializer<T> {
    private aliases: Map<string, string> = new Map();

    private constructor(public readonly name: string, private readonly fields: Map<string, { name: string, schema: JSONSchema7 }>, private readonly required_fields: string[], aliases: Record<string, string>) {
        super(3);

        // Aliases are case-insensitive
        Object.entries(aliases).forEach(([k, v]) => {
            this.aliases.set(k.toLowerCase(), v);
        });

    }

    static from_schema<T extends Record<string, any>>(schema: JSONSchema7, aliases: Record<string, string>): ObjectDeserializer<T> {
        if (schema.type !== "object") {
            throw new Error(`Schema must be of type object`);
        }

        const name = schema.title ?? "Unnamed object";
        const fields = schema.properties ?? {};
        const required_fields = schema.required ?? [];

        const fieldMap = new Map<string, { name: string, schema: JSONSchema7 }>();
        Object.entries(fields).forEach(([k, v]) => {
            if (typeof v === "boolean") {
                throw new Error(`Boolean schema not supported for field ${k}`);
            }
            fieldMap.set(k.toLowerCase(), { name: k, schema: v });
        });

        return new ObjectDeserializer<T>(name, fieldMap, required_fields, aliases);
    }

    copy_with_aliases(aliases: Record<string, string>): ObjectDeserializer<T> {
        this.aliases.forEach((v, k) => {
            if (aliases[k] === undefined) {
                aliases[k] = v;
            }
        });
        return new ObjectDeserializer(this.name, this.fields, this.required_fields, aliases);
    }

    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<T> {
        diagnostics.pushScope(this.name);
        const result: any = {};
        for (const [k, v] of raw.as_dict()) {
            if (!k) {
                diagnostics.pushUnknownWarning("Empty key in object");
                continue;
            }
            const k_as_str = k.as_str(false);
            if (k_as_str === undefined) {
                diagnostics.pushUnknownWarning(`Non-string key in object: ${k.as_self()}`);
                continue;
            }

            const fieldName = this.aliases.get(k_as_str.toLowerCase()) ?? k_as_str;
            const fieldSchema = this.fields.get(fieldName.toLowerCase());

            if (!fieldSchema) {
                diagnostics.pushUnknownWarning(`Unknown field: ${fieldName}`);
                continue;
            }

            const { name, schema } = fieldSchema;

            const fieldDeserializer = fromLut(schema);
            diagnostics.pushScope(name);
            const fieldResult = fieldDeserializer.coerce(v, diagnostics, fromLut);
            diagnostics.popScope(false);

            if (fieldResult.has_value) {
                result[name] = fieldResult.value;
            }
        }

        const missingFields = this.required_fields.filter(x => Object.hasOwn(result, x) === false);
        if (missingFields.length > 0) {
            diagnostics.pushUnknownError(`Missing required fields: ${missingFields.join(", ")}`);
            diagnostics.popScope(false);
            return Result.failed();
        }
        diagnostics.popScope(false);

        // Inject all optional fields that are not present
        this.fields.forEach((v) => {
            if (Object.hasOwn(result, v.name) === false) {
                result[v.name] = null;
            }
        })

        return Result.from_value<T>(result);
    }
}

export { ObjectDeserializer };