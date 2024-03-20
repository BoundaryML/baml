import { JSONSchema7, JSONSchema7Definition } from "json-schema";
import { BaseDeserializer } from "./base_deserializer";
import { NoneDeserializer, PrimitiveDeserializer } from "./primitive_deserializer";
import { EnumDeserializer } from "./enum_deserializer";
import { ObjectDeserializer } from "./object_deserializer";
import { fromValue } from "./raw_wrapper/loader";
import { Diagnostics } from "./diagnostics";
import { ListDeserializer, UnionDeserializer } from "./complex_deserializer";

const DefaultDeserializerLUT = {
    None: new NoneDeserializer(),
    bool: new PrimitiveDeserializer(x => x.as_bool(), "Expected bool", 7),
    int: new PrimitiveDeserializer(x => x.as_int(), "Expected int", 8),
    float: new PrimitiveDeserializer(x => x.as_float(), "Expected float", 9),
    string: new PrimitiveDeserializer<string>(x => x.as_str(false), "Expected str", 1),
}

const GeneratedDeserializerLUT = new Map<string, EnumDeserializer<any> | ObjectDeserializer<any>>();


const registerEnumDeserializer = <T extends Record<string, string>>(schema: JSONSchema7, aliases: Record<string, string>) => {
    const deserializer = EnumDeserializer.from_schema<T>(schema, aliases);
    if (GeneratedDeserializerLUT.has(deserializer.name)) {
        console.warn(`Deserializer for ${deserializer.name} already exists. (If you are in development mode using hot-reloading this may be expected.)`);
    }
    GeneratedDeserializerLUT.set(deserializer.name, deserializer);
}

const registerObjectDeserializer = <T extends Record<string, any>>(schema: JSONSchema7, aliases: Record<string, string>) => {
    const deserializer = ObjectDeserializer.from_schema<T>(schema, aliases);
    if (GeneratedDeserializerLUT.has(deserializer.name)) {
        console.warn(`Deserializer for ${deserializer.name} already exists. (If you are in development mode using hot-reloading this may be expected.)`);
    }
    GeneratedDeserializerLUT.set(deserializer.name, deserializer);
}

class Deserializer<T> {
    private overrides: Map<string, EnumDeserializer<any> | ObjectDeserializer<any>> = new Map();

    constructor(private schema: JSONSchema7, private target: JSONSchema7) {
    }

    // on aliases that are scoped to a specific impl, this needs to be specialized for the constructed deserializer instance
    // for that specific impl
    overload(name: string, aliases: Record<string, string>) {
        if (!GeneratedDeserializerLUT.has(name)) {
            throw new Error(`Deserializer for ${name} not found`);
        }

        const overridden = GeneratedDeserializerLUT.get(name)!.copy_with_aliases(aliases);
        if (this.overrides.has(name)) {
            console.warn(`Overload for ${name} already exists. (If you are in development mode using hot-reloading this may be expected.)`);
        }
        this.overrides.set(name, overridden);
    }

    private getInterface(t: JSONSchema7): JSONSchema7 {
        if (t.$ref) {
            const name = t.$ref.split("/").pop();
            if (name === undefined) {
                throw new Error(`Invalid reference`);
            }
            if (this.schema.definitions === undefined) {
                throw new Error(`No definitions found`);
            }

            const schema = this.schema.definitions[name];
            if (schema === undefined) {
                throw new Error(`Definition ${name} not found`);
            }
            if (typeof schema === "boolean") {
                throw new Error(`Definition ${name} is a boolean`);
            }
            return this.getInterface(schema);
        } else {
            return t;
        }
    }

    get_deserializer(_t: JSONSchema7Definition): BaseDeserializer<any> {
        if (typeof _t === "boolean") {
            throw new Error(`Boolean types are not supported`);
        }
        const t = this.getInterface(_t);
        if (t.type === "object" || t.enum) {
            const name = t.title;
            if (name === undefined) {
                throw new Error(`${t.enum ? "Enum" : "Object"} schema must have a title`);
            }
            const deserializer = this.overrides.get(name) ?? GeneratedDeserializerLUT.get(name);
            if (deserializer === undefined) {
                throw new Error(`Deserializer for ${name} not found`);
            }
            return deserializer;
        } else if (t.type === "string") {
            return DefaultDeserializerLUT.string;
        } else if (t.type === "boolean") {
            return DefaultDeserializerLUT.bool;
        } else if (t.type === "integer") {
            return DefaultDeserializerLUT.int;
        } else if (t.type === "number") {
            return DefaultDeserializerLUT.float;
        } else if (t.type === "null") {
            return DefaultDeserializerLUT.None;
        } else if (t.type === "array") {
            if (t.items === undefined) {
                throw new Error(`Array schema must have items`);
            }
            if (Array.isArray(t.items)) {
                throw new Error(`Tuple types are not supported`);
            }
            const item = this.get_deserializer(t.items);
            return new ListDeserializer(item);
        } else if (t.anyOf) {
            const items = t.anyOf.map(x => this.get_deserializer(x));
            return new UnionDeserializer(items);
        } else if (Array.isArray(t.type)) {
            // This is done for optional types
            const items = t.type.map(x => this.get_deserializer({ type: x }));
            return new UnionDeserializer(items);
        } else {
            throw new Error(`Unsupported schema type: ${JSON.stringify(_t)}\n ${t.type}`);
        }

    }

    coerce(value: string): T {
        const d = new Diagnostics(value);
        const raw = fromValue(value, d);

        const deserializer = this.get_deserializer(this.target);
        const response = deserializer.coerce(raw, d, this.get_deserializer.bind(this));
        d.toException();
        return response.as_value;
    }
}

export { Deserializer, registerEnumDeserializer, registerObjectDeserializer }
