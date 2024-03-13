import { BaseDeserializer, CheckLutFn, Result } from "./base_deserializer";
import { Diagnostics } from "./diagnostics";
import { RawWrapper } from "./raw_wrapper/raw_wrapper";
import { JSONSchema7 } from "json-schema";

// Taken from https://github.com/sindresorhus/escape-string-regexp/blob/main/index.js
const regex_escape = (s: string) => s .replace(/[|\\{}()[\]^$+*?.]/g, '\\$&') .replace(/-/g, '\\x2d');

const count_occurrences = (text: string, searchTerm: string) => {
    const indexes = [];
    let index = text.indexOf(searchTerm);

    while (index !== -1) {
        indexes.push(index);
        index = text.indexOf(searchTerm, index + 1);
    }

    return indexes.length;
}

class EnumDeserializer<T extends Record<string, string>> extends BaseDeserializer<keyof T> {
    private value_names_by_alias: Map<string, string> = new Map();

    private constructor(public readonly name: string, private readonly values: Map<string, keyof T>, aliases: Record<string, string>) {
        super(3);

        // Aliases are case-insensitive
        Object.entries(aliases).forEach(([k, v]) => {
            this.value_names_by_alias.set(k.toLowerCase(), v.toLowerCase());
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
        this.value_names_by_alias.forEach((alias, value_name) => {
            if (!aliases[alias]) {
                aliases[alias] = value_name;
            }
        });
        return new EnumDeserializer(this.name, this.values, aliases);
    }

    *aliases(): IterableIterator<[string, keyof T]> {
        for (const [value_name, value] of this.values.entries()) {
            yield [value_name, value];
        }
        for (const [alias, value_name] of this.value_names_by_alias.entries()) {
            const value = this.values.get(value_name);
            if (value) {
                yield [alias, value];
            }
        }
    }

    *normalized_aliases(): IterableIterator<[string, keyof T]> {
        for (const [value_name, value] of this.values.entries()) {
            yield [value_name.toLowerCase(), value];
        }
        for (const [alias, value_name] of this.value_names_by_alias.entries()) {
            const value = this.values.get(value_name);
            if (value) {
                yield [alias.toLowerCase().replaceAll(/[^a-zA-Z0-9]+/g, ' '), value];
            }
        }
    }

    // Follows rules defined in https://www.notion.so/gloochat/Enum-Deserialization-9608ed24e8d345bcabb8c10ac8b177ad
    coerce(raw: RawWrapper, diagnostics: Diagnostics, fromLut: CheckLutFn<any>): Result<keyof T> {
        diagnostics.pushScope(this.name);
        const parsed = raw.as_smart_str(false)?.trim();
        if (parsed === undefined) {
            diagnostics.pushUnknownError(`Expected string, got ${raw.as_self()}`);
            diagnostics.popScope(false);
            return Result.failed();
        }

        const search = (contents: string, aliases: () => IterableIterator<[string, keyof T]>): keyof T | undefined => {
            for (const [alias, value] of aliases()) {
                if (contents === alias) {
                    return value;
                }
            }

            for (const [alias, value] of aliases()) {
                if (contents.endsWith(`: ${alias}`)) {
                    return value;
                }
                if (contents.endsWith(`\n\n${alias}`)) {
                    return value;
                }
            }

            // TODO: uncomment when descriptions are wired through
            // for (const [alias, value] of this.aliases()) {
            //     const description = "";
            //     const matches = [...contents.matchAll(new RegExp(`${regex_escape(alias)}[^a-zA-Z0-9]{1,5}${regex_escape(description)}`, 'g'))];
            //     if (matches.length > 0) {
            //         return value;
            //     }
            // }
        }

        const value = search(parsed.toLowerCase(),() => this.aliases());
        if (value) {
            diagnostics.popScope(true);
            return Result.from_value(value);
        }

        const value2 = search(parsed.toLowerCase().replaceAll(/[^a-zA-Z0-9]+/g, ' '), () => this.normalized_aliases());
        if (value2) {
            diagnostics.popScope(true);
            return Result.from_value(value2);
        }

        const find_most_common = (contents: string, aliases: () => IterableIterator<[string, keyof T]>): keyof T | undefined => {
            let most_freq_match = undefined;
            for (const [alias, value] of this.aliases()) {
                const match = {
                    alias,
                    value,
                    count: count_occurrences(contents, alias),
                    matches_with_count: 1,
                };

                if (most_freq_match === undefined || match.count > most_freq_match.count) {
                    most_freq_match = match;
                } else if (match.count === most_freq_match.count) {
                    most_freq_match.matches_with_count++;
                }
            }
            if (most_freq_match !== undefined) {
                if (most_freq_match.matches_with_count === 1) {
                    return most_freq_match.value;
                }
            }
        };

        const most_common = find_most_common(parsed.toLowerCase(), () => this.aliases());
        if (most_common) {
            diagnostics.popScope(true);
            return Result.from_value(most_common);
        }
        const most_common2 = find_most_common(parsed.toLowerCase().replaceAll(/[^a-zA-Z0-9]+/g, ' '), () => this.normalized_aliases());
        if (most_common2) {
            diagnostics.popScope(true);
            return Result.from_value(most_common2);
        }

        diagnostics.pushUnknownError(`Unknown enum value: ${parsed}`);
        diagnostics.popScope(false);
        return Result.failed();
    }
}

export { EnumDeserializer };