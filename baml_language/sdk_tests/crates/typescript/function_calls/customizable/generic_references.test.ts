// A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`): an
// instance crosses as a reference that carries its type arguments, so its
// methods bind `T` from the receiver, and one engine instance is one wrapper.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { _ParseCache, _new_parse_cache } from "./baml_sdk/baml/sap/index.js";
import { identity } from "./baml_sdk/generic_tests/index.js";

describe("function_calls — generic references", () => {
  it("generic_references_method_binds_the_class_type_argument_from_its_receiver", async () => {
    const ints = _new_parse_cache({ $types: { T: "int" } });
    expect(ints).toBeInstanceOf(_ParseCache);
    expect(ints._parse_final("41")).toBe(41);
    expect(await ints._parse_final_async("41")).toBe(41);
    // The same text read at another type argument: the binding is the
    // receiver's, not a default.
    const strings = _new_parse_cache({ $types: { T: "string" } });
    expect(strings._parse_final("41")).toBe("41");
  });

  it("generic_references_one_engine_instance_is_one_wrapper", () => {
    const cache = _new_parse_cache({ $types: { T: "int" } });
    expect(identity(cache)).toBe(cache);
    expect(_new_parse_cache({ $types: { T: "int" } })).not.toBe(cache);
  });
});
