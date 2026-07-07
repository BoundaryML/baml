// Roundtrip coverage for baml_sdk/interfaces — interface types (BEP-044).
// Counterpart of python_pydantic2/.../roundtrip_tests/test_interfaces.py.
//
// Interfaces are BAML-side contracts, not serializable host SDK models:
// codegen surfaces interface-typed boundary positions as `unknown`
// (client_codegen.rs: `TirTy::Interface(..) => cg::Ty::BuiltinUnknown`).
//
// KNOWN GAP (nodejs bridge) — unlike the python SDK, where all of these
// scenarios work: outbound host→engine encoding here is driven by the
// declared codegen type, so a class instance passed in an interface
// position (`unknown`) loses its class identity and encodes as a plain
// object. The engine then cannot dispatch interface methods on it ("VM
// internal error: virtual call could not resolve interface method"), and
// round-tripped interface positions come back as plain objects rather
// than class instances. Interface *returns* are unaffected: the value
// leaves the engine carrying its concrete class, so the host receives a
// real class instance. The tests below pin this *current* behavior, not
// desired behavior — update them when the nodejs bridge encodes by the
// value's runtime class (as python does) or codegen grows a structural
// interface representation.
import "./baml_sdk/index.js"; // initializes the BAML runtime
import { describe, it, expect } from "vitest";
import {
  Circle,
  Rect,
  ShapeBox,
  Square,
  box_area,
  return_circle_as_shape,
  return_rect_as_shape,
  return_square_as_shape,
  round_trip_optional_shape,
  round_trip_shape,
  round_trip_shape_box,
  round_trip_shape_list,
  shape_area,
  shape_area_async,
  sum_areas,
} from "./baml_sdk/interfaces/index.js";

describe("roundtrip interfaces", () => {
  // ── return position: host receives the concrete implementing class ──

  it("return_square_as_shape", () => {
    const s = return_square_as_shape();
    expect(s).toBeInstanceOf(Square);
    expect((s as Square).side).toBeCloseTo(5);
  });

  it("return_rect_as_shape", () => {
    const s = return_rect_as_shape();
    expect(s).toBeInstanceOf(Rect);
    expect((s as Rect).width).toBeCloseTo(3);
    expect((s as Rect).height).toBeCloseTo(4);
  });

  it("return_circle_as_shape (out-of-body impl)", () => {
    const s = return_circle_as_shape();
    expect(s).toBeInstanceOf(Circle);
    expect((s as Circle).radius).toBeCloseTo(2);
  });

  // ── parameter position — KNOWN GAP: class identity lost on encode, so
  // the engine cannot dispatch interface methods on host instances ─────

  it("shape_area panics on Square (class identity lost)", () =>
    expect(() => shape_area(new Square({ side: 5 }))).toThrow(
      /could not resolve interface method/,
    ));

  it("shape_area panics on Rect", () =>
    expect(() => shape_area(new Rect({ width: 3, height: 4 }))).toThrow(
      /could not resolve interface method/,
    ));

  it("shape_area panics on out-of-body impl", () =>
    expect(() => shape_area(new Circle({ radius: 2 }))).toThrow(
      /could not resolve interface method/,
    ));

  it("shape_area_async rejects", async () =>
    await expect(shape_area_async(new Square({ side: 6 }))).rejects.toThrow(
      /could not resolve interface method/,
    ));

  it("sum_areas panics", () =>
    expect(() =>
      sum_areas(new Square({ side: 2 }), new Rect({ width: 3, height: 4 })),
    ).toThrow(/could not resolve interface method/));

  // ── round trips — KNOWN GAP: interface positions come back as plain
  // objects (field values survive, class identity does not) ────────────

  it("round_trip_shape returns a plain object", () => {
    const r = round_trip_shape(new Rect({ width: 2, height: 3 }));
    expect(r).not.toBeInstanceOf(Rect);
    expect(r).toEqual({ width: 2, height: 3 });
  });

  it("round_trip_shape_list returns plain objects", () => {
    const r = round_trip_shape_list([
      new Square({ side: 1 }),
      new Rect({ width: 2, height: 3 }),
      new Circle({ radius: 4 }),
    ]);
    expect(r[0]).not.toBeInstanceOf(Square);
    expect(r).toEqual([{ side: 1 }, { width: 2, height: 3 }, { radius: 4 }]);
  });

  it("round_trip_optional_shape null", () =>
    expect(round_trip_optional_shape(null)).toBeNull());

  it("round_trip_optional_shape value returns a plain object", () => {
    const r = round_trip_optional_shape(new Circle({ radius: 1 }));
    expect(r).not.toBeInstanceOf(Circle);
    expect(r).toEqual({ radius: 1 });
  });

  it("round_trip_shape_box keeps the box, loses the field's class", () => {
    // The box itself is a declared class type, so its identity survives;
    // its interface-typed `shape` field decays to a plain object.
    const r = round_trip_shape_box(new ShapeBox({ shape: new Square({ side: 2 }) }));
    expect(r).toBeInstanceOf(ShapeBox);
    expect(r.shape).not.toBeInstanceOf(Square);
    expect(r.shape).toEqual({ side: 2 });
  });

  it("box_area panics (field dispatch)", () =>
    expect(() =>
      box_area(new ShapeBox({ shape: new Rect({ width: 2, height: 5 }) })),
    ).toThrow(/could not resolve interface method/));

  // ── KNOWN GAPS shared with the python SDK ────────────────────────────

  it("non-implementor panics at dispatch, not encode", () => {
    // No encode-time conformance check for interface-typed params: a
    // non-implementing value encodes fine and only fails inside the VM.
    expect(() =>
      shape_area(new ShapeBox({ shape: new Square({ side: 1 }) })),
    ).toThrow(/could not resolve interface method/);
    expect(() => shape_area(42)).toThrow(/could not resolve interface method/);
  });

  it("impl-method host binding is not callable", () => {
    // sdkgen emits `area`/`area_async` bindings on implementing classes
    // (named `user.interfaces.Square.area`), but the engine registers
    // interface-impl methods under a synthetic `Shape$for$Square` name,
    // so calling the binding from the host panics with "Function not
    // found". Update this pin when the binding and registration agree.
    expect(() => new Square({ side: 3 }).area()).toThrow(/Function not found/);
  });
});
