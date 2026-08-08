import { BamlError, BamlType } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";

import {
  HostParse,
  HostTypeEqual,
  HostTypeName,
  StaticChoice,
  StaticNamed,
  StaticRecord,
  reflect,
} from "./baml_sdk/index.js";

describe("BEP-066 TypeScript host boundary", () => {
  it("runtime enum definitions cross and decode loosely", () => {
    const red = reflect.enum.value("RED", { alias: "k7", description: "warm" });
    const category = reflect.enum.new("Category", [red, "BLUE"]);

    expect(category).toBeInstanceOf(BamlType);
    expect(HostParse<string>('"k7"', { $types: { T: category } })).toBe("RED");
  });

  it("runtime classes preserve field order, metadata, nesting, and loose reads", () => {
    const category = reflect.enum.new("Category", [
      reflect.enum.value("RED", { alias: "k7" }),
    ]);
    const record = reflect.class.new("RuntimeRecord", {
      label: reflect.type.of(String).meta({ alias: "display_label" }),
      category,
      scores: reflect.type.of("int").array(),
    });

    const parsed = HostParse<Record<string, unknown>>(
      '{"display_label":"ok","category":"k7","scores":[1,2]}',
      { $types: { T: record } },
    );
    expect(parsed).toEqual({ label: "ok", category: "RED", scores: [1, 2] });
    expect(Object.getPrototypeOf(parsed)).toBeNull();
    expect(Object.keys(parsed)).toEqual(["label", "category", "scores"]);
  });

  it("compiled packages return class definitions with their full graph", () => {
    const pkg = reflect.Package.compile({
      "runtime.baml": "class CompiledRow { amount int note string? }",
    });
    const compiled = pkg.get_class("CompiledRow");

    expect(compiled).toBeInstanceOf(BamlType);
    expect(
      HostParse<Record<string, unknown>>('{"amount":7}', {
        $types: { T: compiled! },
      }),
    ).toEqual({ amount: 7, note: null });
  });

  it("fresh-mints every wire occurrence and rejects serialization", () => {
    const runtimeType = reflect.class.new("Fresh", {
      value: reflect.type.of("int"),
    });
    expect(HostTypeEqual({ $types: { A: runtimeType, B: runtimeType } })).toBe(false);
    expect(() => JSON.stringify(runtimeType)).toThrow(/cannot be serialized/);
  });

  it("accepts known builtins, recursive shapes, generated tokens, and subclasses", () => {
    class ChildRecord extends StaticRecord {}

    expect(HostTypeName({ $types: { T: String } })).toBe("string");
    expect(HostTypeName({ $types: { T: { list: { optional: String } } } })).toBe(
      "(string | null)[]",
    );
    expect(HostTypeName({ $types: { T: ChildRecord } })).toBe("StaticRecord");
    expect(HostTypeName({ $types: { T: StaticChoice } })).toBe("StaticChoice");
    expect(HostTypeName({ $types: { T: StaticNamed } })).toBe("StaticNamed");
    expect(reflect.type.of("int").optional().array()).toBeInstanceOf(BamlType);
    expect(reflect).toHaveProperty("class");
    expect(reflect).toHaveProperty("type");

    class NotGenerated {}
    expect(() => HostTypeName({ $types: { T: NotGenerated } })).toThrow(
      /unsupported TypeScript type token/,
    );
  });

  it("keeps handles composition-only and preserves typed reflection errors", () => {
    const runtimeType = reflect.type.of("int");
    expect("kind" in runtimeType).toBe(false);
    expect("fields" in runtimeType).toBe(false);
    expect("as_type" in runtimeType).toBe(false);

    let thrown: unknown;
    try {
      reflect.Package.compile({ "broken.baml": "class {" });
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeInstanceOf(BamlError);
    expect((thrown as BamlError).className).toBe("baml.reflect.errors.CompilationError");
  });
});
