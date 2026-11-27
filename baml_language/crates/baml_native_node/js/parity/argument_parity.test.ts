// How arguments are read, the same on both sides: this file runs against
// today's SDK and against the native bindings (crates/baml_native_node).
import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import {
  Mood,
  Note,
  Pair,
  Point,
  add,
  count_map,
  float_or_literal,
  mood_or_text,
  note_or_tag,
  point_or_map,
  round_trip_point,
} from "./baml_sdk/bridge_test/index.js";
import { OptionalContainer, round_trip_optional_container } from "./baml_sdk/optional/index.js";

/** A refused argument: a plain Error with the code InvalidArg, and `message`. */
const refused = async (call: Promise<unknown>, message: string): Promise<void> => {
  const error = await call.then(
    () => undefined,
    (e: unknown) => e,
  );
  expect(error).toBeInstanceOf(Error);
  expect((error as { code?: string }).code).toBe("InvalidArg");
  expect((error as Error).message).toBe(message);
};

const POINT = "user.bridge_test.round_trip_point: argument `p`";

describe("argument parity", () => {
  it("parity_class_takes_its_own_instance_or_a_plain_object", async () => {
    expect(await round_trip_point(new Point({ x: 1, y: 2 }))).toEqual(new Point({ x: 1, y: 2 }));
    expect(await round_trip_point({ x: 1, y: 2 })).toEqual(new Point({ x: 1, y: 2 }));
    await refused(
      round_trip_point(new Pair({ x: 1, y: 2 })),
      `${POINT}: expected bridge_test.Point, found Pair`,
    );
  });

  it("parity_unknown_key_is_refused", async () => {
    await refused(
      // @ts-expect-error `z` is no field of Point
      round_trip_point({ x: 1, y: 2, z: 3 }),
      `${POINT}: class user.bridge_test.Point has no field \`z\``,
    );
  });

  it("parity_only_own_properties_are_read", async () => {
    await refused(
      round_trip_point(Object.create({ x: 1, y: 2 })),
      `${POINT}: expected bridge_test.Point, found object with 0 keys`,
    );
    await refused(
      // @ts-expect-error a Date is no OptionalContainer, even with every field nullable
      round_trip_optional_container(new Date()),
      "user.optional.round_trip_optional_container: argument `c`: expected optional.OptionalContainer, found Date",
    );
    expect(await round_trip_optional_container(new OptionalContainer({}))).toEqual(
      new OptionalContainer({ optional_int: null, optional_class: null, optional_union: null }),
    );
  });

  it("parity_a_map_takes_a_plain_object_not_a_map", async () => {
    expect(await count_map({ a: 1, b: 2 })).toBe(2);
    await refused(
      // @ts-expect-error a Map is not a map<string, int> here
      count_map(new Map([["a", 1]])),
      "user.bridge_test.count_map: argument `m`: expected map<string, int>, found Map",
    );
  });

  it("parity_an_int_takes_a_bigint_in_range", async () => {
    // @ts-expect-error a bigint where a number is declared
    expect(await add(10n, 1)).toBe(11);
    await refused(
      // @ts-expect-error as above
      add(2n ** 70n, 1),
      "user.bridge_test.add: argument `a`: expected int, found bigint 1180591620717411303424",
    );
  });

  it("parity_union_members_are_ranked", async () => {
    expect(await mood_or_text("Happy")).toBe("string");
    expect(await mood_or_text(Mood.Happy)).toBe("string");
    expect(await float_or_literal(1)).toBe("int");
    expect(await float_or_literal(1.5)).toBe("float");
    // 3 is neither literal: the float takes it.
    expect(await float_or_literal(3)).toBe("float");
    expect(await point_or_map({ x: 1, y: 2 })).toBe("map");
    expect(await point_or_map(new Point({ x: 1, y: 2 }))).toBe("class");
    // The fewest fields read as null wins: {text} is a Tag.
    expect(await note_or_tag({ text: "t" })).toBe("tag");
    expect(await note_or_tag({ text: "t", meta: "m" })).toBe("note");
    expect(await note_or_tag(new Note({ text: "t", meta: null }))).toBe("note");
  });
});
