import { JSONSchema7 } from "json-schema";
import { Deserializer, registerObjectDeserializer } from "../src/deserializer/deserializer";
import { Category } from "./test_helpers";


describe("String Deserializer", () => {
    const deserializer = new Deserializer<string>({
        definitions: {}
    }, {
        type: "string"
    });
    test("string_from_string", () => {
        expect(deserializer.coerce("hello")).toBe("hello");
    });

    test("string_from_str_w_quotes", () => {
        expect(deserializer.coerce("\"hello\"")).toBe("\"hello\"");
    });

    test("string_from_object", () => {
        const obj = { hello: "world" }
        expect(deserializer.coerce(JSON.stringify(obj))).toBe(JSON.stringify(obj));
    });

    test("string_from_obj_and_string", () => {
        const test_str = 'The output is: {"hello": "world"}';
        expect(deserializer.coerce(test_str)).toBe(test_str);
    });

    test("string_from_obj_and_string_unbalanced_delims", () => {
        const test_str = 'The output is: {"hello": " {{user_name}world"}';
        expect(deserializer.coerce(test_str)).toBe(test_str);
    });


    test("string_from_list", () => {
        const test_list = ["hello", "world"];
        expect(deserializer.coerce(JSON.stringify(test_list))).toBe(JSON.stringify(test_list));
    });

    test("string_from_int", () => {
        expect(deserializer.coerce("1")).toBe("1");
    });
});

describe("Enum Deserializer", () => {
    const schema: JSONSchema7 = {
        definitions: {
            "Category": {
                type: "string",
                title: "Category",
                enum: ["ONE", "TWO"]
            }
        }
    }

    test("enum_from_string", () => {
        const deserializer = new Deserializer<Category>(schema, {
            $ref: "#/definitions/Category"
        });
        expect(deserializer.coerce("ONE")).toBe(Category.ONE);
        expect(deserializer.coerce("one")).toBe(Category.ONE);
        expect(() => deserializer.coerce("citronella")).toThrow();
    });

    test("enum_from_str_w_quotes", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        expect(deserializer.coerce("\"ONE\"")).toBe(Category.ONE);
    });

    test("enum_missing", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        expect(() => deserializer.coerce("THREE")).toThrow();
    });

    test("enum_with_text_before", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        expect(deserializer.coerce("The output is: ONE")).toBe(Category.ONE);
    });

    test("enum_from_enum_list_single", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        expect(deserializer.coerce('["ONE"]')).toEqual(Category.ONE);
    });

    test("enum_from_enum_list_multi", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        expect(() => deserializer.coerce('["ONE", "TWO"]')).toThrow();
    });

    test("enum_list_from_list", () => {
        const deserializer = new Deserializer<Category[]>(schema, {
            type: 'array',
            items: { $ref: "#/definitions/Category" }
        });
        expect(deserializer.coerce('["ONE"]')).toEqual([Category.ONE]);
    });

    test("enum_list_from_list_multi", () => {
        const deserializer = new Deserializer<Category[]>(schema, {
            type: 'array',
            items: { $ref: "#/definitions/Category" }
        });
        expect(deserializer.coerce('["ONE", "TWO"]')).toEqual([Category.ONE, Category.TWO]);
    });

    test("enum_list_from_list_multi_extra", () => {
        const deserializer = new Deserializer<Category[]>(schema, {
            type: 'array',
            items: { $ref: "#/definitions/Category" }
        });
        expect(deserializer.coerce('["ONE", "THREE", "TWO"]')).toEqual([Category.ONE, Category.TWO]);
    });

    test("test_enum_from_string_with_extra_text_after", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        expect(deserializer.coerce("ONE is the output")).toBe(Category.ONE);
        expect(() => deserializer.coerce("ONE - is the answer, not TWO")).toThrow();
        expect(() => deserializer.coerce("ONE. is the answer, not TWO")).toThrow();
        expect(() => deserializer.coerce("ONE: is the answer, not TWO")).toThrow();
    });

    test("test_enum_alias_from_string", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        deserializer.overload('Category', { 'uno': Category.ONE, 'deux': Category.TWO });

        expect(deserializer.coerce("uno")).toBe(Category.ONE);
        expect(deserializer.coerce("uno is the output")).toBe(Category.ONE);
        expect(deserializer.coerce("deux")).toBe(Category.TWO);
        expect(deserializer.coerce("deux is the output")).toBe(Category.TWO);
    });

    test("test_enum_alias_from_chain_of_thought", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        deserializer.overload('Category', { 'uno': Category.ONE, 'deux': Category.TWO });

        expect(deserializer.coerce("chain of thought\n\nsome more reasoning\n\nuno\n")).toBe(Category.ONE);
        expect(deserializer.coerce("chain of thought\n\nsome more reasoning\n\nAnswer: deux\n")).toBe(Category.TWO);
    });

    // TODO: handle snake case aliases
    test("test_enum_alias_with_punctuation_from_string", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        deserializer.overload('Category', { 'a single item': Category.ONE, 'the:category.is/pair': Category.TWO });

        // people put spaces, dots, slashes, hyphens, underscores in their aliases - we should handle them all
        expect(deserializer.coerce("a single item is the answer")).toBe(Category.ONE);
        expect(deserializer.coerce("a-single-item is the answer")).toBe(Category.ONE);
        expect(deserializer.coerce("the category is pair")).toBe(Category.TWO);
        expect(deserializer.coerce("the_category_is_pair")).toBe(Category.TWO);
    });

    test("test_enum_alias_based_on_max_count", () => {
        const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
        deserializer.overload('Category', { 'uno': Category.ONE, 'deux': Category.TWO });

        expect(() => deserializer.coerce('sorry dave, not sure if the answer is uno or deux')).toThrow();
        expect(deserializer.coerce('the answer is uno: it is clearly uno, definitely not deux')).toBe(Category.ONE);
    });

    test("test_enum_alias_list_from_string", () => {
        const deserializer = new Deserializer<Category[]>(schema, {
            type: 'array',
            items: { $ref: "#/definitions/Category" }
        });
        deserializer.overload('Category', { 'uno': Category.ONE, 'deux': Category.TWO });

        expect(deserializer.coerce('["uno", "deux"]')).toEqual(expect.arrayContaining([Category.ONE, Category.TWO]));
    });

    // test("test_enum_aliases_from_string_with_extra_text", () => {
    //     const deserializer = new Deserializer<Category>(schema, { $ref: "#/definitions/Category" });
    //     expect(deserializer.coerce("ONE is the output")).toBe(Category.ONE);
    //     expect(deserializer.coerce("ONE - is the answer, not TWO")).toBe(Category.ONE);
    // });
});


interface BasicObj {
    foo: string;
}

registerObjectDeserializer({
    title: "BasicObj",
    type: "object",
    properties: {
        foo: {
            type: "string"
        }
    },
    required: ["foo"]
}, {})

describe("Object Deserializer", () => {
    const schema: JSONSchema7 = {
        definitions: {
            BasicObj: {
                title: "BasicObj",
                type: "object",
                properties: {
                    foo: {
                        type: "string"
                    }
                },
                required: ["foo"]
            }
        }
    };

    test("obj_from_str", () => {
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        const test_obj = { foo: "bar" };
        expect(deserializer.coerce(JSON.stringify(test_obj))).toEqual(test_obj);
    });

    test("obj_from_str_with_other_text", () => {
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        expect(deserializer.coerce('The output is: {"foo": "bar"}')).toEqual({ foo: "bar" });
    });

    test("obj_from_str_with_quotes", () => {
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        expect(deserializer.coerce('{"foo": "[\\"bar\\"]"}')).toEqual({ foo: JSON.stringify(["bar"], undefined, 2) });
    });

    // TODO?: this fails
    // test("obj_from_str_with_newlines", () => {
    //     const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
    //     expect(deserializer.coerce('{"foo": "[\\"ba\nr\\"]"}')).toEqual({ foo: JSON.stringify("[\"ba\nr\"]", undefined, 2) });
    // });

    test("obj_from_str_with_newlines_with_quotes", () => {
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        expect(deserializer.coerce('{"foo": "[\\"ba\\nr\\"]"}')).toEqual({ foo: JSON.stringify(["ba\nr"], undefined, 2) });
    })

    test("obj_from_str_with_nested_json_string", () => {
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        expect(deserializer.coerce('{"foo": "{\\"foo\\": [\\"bar\\"]}"}')).toEqual({ foo: '{\n  "foo": [\n    "bar"\n  ]\n}' });
    });

    test("obj_from_str_with_nested_complex_string2", () => {
        const test_value = `Here is how you can build the API call:
\`\`\`json
{
    "foo": {
        "foo": [
            "bar"
        ]
    }
}
\`\`\`
`;
        const deserializer = new Deserializer<string>(schema, { type: "string" });
        expect(deserializer.coerce(test_value)).toEqual(test_value);
    });

    test("obj_from_str_with_string_foo", () => {
        const test_value = `Here is how you can build the API call:
\`\`\`json
{
    "hello": {
        "world": [
            "bar"
        ]
    }
}
\`\`\`
`;
        // Note LLM should add these (\\) too for the value of foo.
        const test_value_str = test_value.replaceAll("\n", "\\n").replaceAll('"', '\\"');

        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });

        expect(deserializer.coerce(`{"foo": "${test_value_str}"}`)).toEqual({ foo: test_value });
    });



    test("json_thing", () => {
        const llm_value = `{
    "foo": "This is a sample string with **markdown** that includes a JSON blob: \`{\\"name\\": \\"John\\", \\"age\\": 30}\`. Please note that the JSON blob inside the string is escaped to fit into the string type."
}`;
        const expected = JSON.parse(llm_value);
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        expect(deserializer.coerce(llm_value)).toEqual(expected);
    });

    test("missing_field", () => {
        const deserializer = new Deserializer<BasicObj>(schema, { $ref: "#/definitions/BasicObj" });
        expect(() => deserializer.coerce("{ 'bar': 'test' }")).toThrow();
    })
});

interface ObjOptionals {
    foo: string | null;
}

registerObjectDeserializer({
    title: "ObjOptionals",
    type: "object",
    properties: {
        foo: {
            type: ["string", "null"],
            default: null
        }
    }
}, {})

describe("Object Deserializer with Optionals", () => {
    const schema: JSONSchema7 = {
        definitions: {
            ObjOptionals: {
                title: "ObjOptionals",
                type: "object",
                properties: {
                    foo: {
                        type: ["string", "null"],
                        default: null
                    }
                }
            }
        }
    };

    test("obj_with_empty_input", () => {
        const deserializer = new Deserializer<ObjOptionals>(schema, { $ref: "#/definitions/ObjOptionals" });
        const obj = {
            "foo": null,
        }
        expect(deserializer.coerce(JSON.stringify(obj))).toEqual(obj);
        expect(deserializer.coerce(JSON.stringify({}))).toEqual(obj);
    });
});

interface BasicClass2 {
    one: string;
    two: string;
}

registerObjectDeserializer({
    title: "BasicClass2",
    type: "object",
    properties: {
        one: {
            type: "string"
        },
        two: {
            type: "string"
        }
    },
    required: ["one", "two"]
}, {})

describe("Object Deserializer with Markdown", () => {
    const schema: JSONSchema7 = {
        definitions: {
            BasicClass2: {
                title: "BasicClass2",
                type: "object",
                properties: {
                    one: {
                        type: "string"
                    },
                    two: {
                        type: "string"
                    }
                },
                required: ["one", "two"]
            }
        }
    };

    test("object_from_str_with_quotes", () => {
        const deserializer = new Deserializer<BasicClass2>(schema, { $ref: "#/definitions/BasicClass2" });
        const test_obj = {
            "one": "hello 'world'",
            "two": 'double hello "world"',
        }
        expect(deserializer.coerce(JSON.stringify(test_obj))).toEqual(test_obj);
    });

    test("obj_from_json_markdown", () => {
        const test_value = `Here is how you can build the API call:
\`\`\`json
{
    "one": "hi",
    "two": "hello"
}
\`\`\`

\`\`\`json
    {
        "test2": {
            "key2": "value"
        },
        "test21": [
        ]    
    }
\`\`\`
`;
        const deserializer = new Deserializer<BasicClass2>(schema, { $ref: "#/definitions/BasicClass2" });
        const res = deserializer.coerce(test_value);
        expect(res).toEqual({
            one: "hi",
            two: "hello"
        });
    });
});

interface BasicWithList {
    a: number;
    b: string;
    c: string[];
}

registerObjectDeserializer({
    title: "BasicWithList",
    type: "object",
    properties: {
        a: {
            type: "integer"
        },
        b: {
            type: "string"
        },
        c: {
            type: "array",
            items: {
                type: "string"
            }
        }
    },
    required: ["a", "b", "c"]
}, {})

describe("Object Deserializer with List", () => {
    const schema: JSONSchema7 = {
        definitions: {
            BasicWithList: {
                title: "BasicWithList",
                type: "object",
                properties: {
                    a: {
                        type: "integer"
                    },
                    b: {
                        type: "string"
                    },
                    c: {
                        type: "array",
                        items: {
                            type: "string"
                        }
                    }
                },
                required: ["a", "b", "c"]
            }
        }
    };

    test("complex_obj_from_string", () => {
        const deserializer = new Deserializer<BasicWithList>(schema, { $ref: "#/definitions/BasicWithList" });
        const test_obj = {
            "a": 1,
            "b": "hello",
            "c": ["world"],
        }
        const res = deserializer.coerce(JSON.stringify(test_obj));
        expect(res).toEqual(test_obj);
    });
});


interface Child {
    hi: string;
}

registerObjectDeserializer({
    title: "Child",
    type: "object",
    properties: {
        hi: {
            type: "string"
        }
    },
    required: ["hi"]
}, {})

interface Parent {
    child: Child;
}

registerObjectDeserializer({
    title: "Parent",
    type: "object",
    properties: {
        child: {
            $ref: "#/definitions/Child"
        }
    },
    required: ["child"]
}, {})

describe("Complex Object Deserializer", () => {
    const schema: JSONSchema7 = {
        definitions: {
            Child: {
                title: "Child",
                type: "object",
                properties: {
                    hi: {
                        type: "string"
                    }
                },
                required: ["hi"]
            },
            Parent: {
                title: "Parent",
                type: "object",
                properties: {
                    child: {
                        $ref: "#/definitions/Child"
                    }
                },
                required: ["child"]
            }
        }
    };

    test("complex_obj_from_string", () => {
        const deserializer = new Deserializer<Parent>(schema, { $ref: "#/definitions/Parent" });
        const test_obj = {
            "child": { "hi": "hello" }
        }
        const res = deserializer.coerce(JSON.stringify(test_obj));
        expect(res).toEqual(test_obj);
    });

    test("complex_obj_from_string_json_markdown", () => {
        const deserializer = new Deserializer<Parent>(schema, { $ref: "#/definitions/Parent" });
        const test_str = `Here is how you can build the API call:
{
    "child": {
        "hi": "hello"
    }
}
`;
        const res = deserializer.coerce(test_str);
        expect(res).toEqual({
            child: {
                hi: "hello"
            }
        });
    });
});

/*

def test_list_from_string() -> None:
    deserializer = Deserializer[List[str]](List[str])
    test_obj = ["hello", "world"]
    res = deserializer.from_string(json.dumps(test_obj))
    assert res == ["hello", "world"]


def test_list_object_from_string() -> None:
    deserializer = Deserializer[List[BasicClass]](List[BasicClass])
    test_obj = [{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]
    res = deserializer.from_string(json.dumps(test_obj))
    assert res == [BasicClass(a=1, b="hello"), BasicClass(a=2, b="world")]
*/

interface BasicClass {
    a: number;
    b: string;
}

registerObjectDeserializer({
    title: "BasicClass",
    type: "object",
    properties: {
        a: {
            type: "integer"
        },
        b: {
            type: "string"
        }
    },
    required: ["a", "b"]
}, {
});

describe("List Deserializer", () => {
    const schema: JSONSchema7 = {
        definitions: {
            BasicClass: {
                title: "BasicClass",
                type: "object",
                properties: {
                    a: {
                        type: "integer"
                    },
                    b: {
                        type: "string"
                    }
                },
                required: ["a", "b"]
            }
        }
    };

    test("list_from_string", () => {
        const deserializer = new Deserializer<string[]>(schema, { type: "array", items: { type: "string" } });
        const test_obj = ["hello", "world"];
        const res = deserializer.coerce(JSON.stringify(test_obj));
        expect(res).toEqual(test_obj);
    });

    test("list_object_from_string", () => {
        const deserializer = new Deserializer<BasicClass[]>(schema, { type: "array", items: { $ref: "#/definitions/BasicClass" } });
        const test_obj = [{ "a": 1, "b": "hello" }, { "a": 2, "b": "world" }];
        const res = deserializer.coerce(JSON.stringify(test_obj));
        expect(res).toEqual([
            { a: 1, b: "hello" },
            { a: 2, b: "world" }
        ]);
    });
});

describe("Union Deserializer", () => {
    const schema: JSONSchema7 = {
        definitions: {
            BasicClass: {
                title: "BasicClass",
                type: "object",
                properties: {
                    a: {
                        type: "integer"
                    },
                    b: {
                        type: "string"
                    }
                },
                required: ["a", "b"]
            }
        }
    };

    test("union_from_string", () => {
        const deserializer = new Deserializer<string | number>(schema, { anyOf: [{ type: "string" }, { type: "integer" }] });
        expect(deserializer.coerce("hello")).toBe("hello");
        // Since string is first, it will be the default.
        expect(deserializer.coerce("1")).toBe("1");

        const deserializer2 = new Deserializer<string | number>(schema, { anyOf: [{ type: "integer" }, { type: "string" }] });
        expect(deserializer2.coerce("1")).toBe(1);
    });

    test("union_fail", () => {
        const deserializer = new Deserializer<string | number>(schema, { anyOf: [{ type: "integer" }, { type: "number" }] });
        expect(() => deserializer.coerce("hello1")).toThrow();
    });

    test("union_from_string_with_quotes", () => {
        const deserializer = new Deserializer<string | number>(schema, { anyOf: [{ type: "string" }, { type: "integer" }] });
        expect(deserializer.coerce("\"hello\"")).toBe("\"hello\"");
    });

    test("union_from_object", () => {
        const obj = { hello: "world" }
        const deserializer = new Deserializer<string | number>(schema, { anyOf: [{ type: "string" }, { type: "integer" }] });
        expect(deserializer.coerce(JSON.stringify(obj))).toBe(JSON.stringify(obj, null, 2));
    });

    test("union_from_obj_and_string_string_first", () => {
        const test_str = 'The output is: {"a": 19, "b": "hello"}';
        const deserializer = new Deserializer<string | number>(schema, { anyOf: [{ type: "string" }, { type: "integer" }, { $ref: "#/definitions/BasicClass" }] });
        expect(deserializer.coerce(test_str)).toBe(test_str);
    });

    test("union_from_obj_and_string_obj_first", () => {
        const test_str = 'The output is: {"a": 19, "b": "hello"}';
        const deserializer = new Deserializer<string | number>(schema, { anyOf: [{ $ref: "#/definitions/BasicClass" }, { type: "string" }, { type: "integer" }] });
        expect(deserializer.coerce(test_str)).toEqual({ a: 19, b: "hello" });
    });
});