// Coverage for interface types (BEP-044) at the host SDK boundary.
//
// Interfaces are BAML-side contracts, not serializable host SDK models:
// codegen surfaces interface-typed boundary positions as opaque values
// (`typing.Any` / `unknown` — see `client_codegen.rs`
// `TirTy::Interface(..) => cg::Ty::BuiltinUnknown`). The *values* crossing
// the boundary are always concrete class instances, so what these fixtures
// pin is:
//
//   - return position: the host receives the concrete implementing class
//   - parameter position: the host passes a concrete instance and the
//     engine dispatches interface methods on it
//   - encode → FFI → decode round trips preserve the concrete class, in
//     bare, list, optional, and class-field positions
//
// The two SDKs currently diverge on the inbound direction: the python
// bridge encodes host instances by their runtime class (all of the above
// works), while the nodejs bridge encodes by the declared codegen type
// (`unknown`), losing class identity in interface positions. Each SDK's
// test file pins its own current behavior.

interface Shape {
  function area(self) -> int
}

// In-body `implements` form.
class Square {
  side int
  implements Shape {
    function area(self) -> int {
      self.side * self.side
    }
  }
}

class Rect {
  width int
  height int
  implements Shape {
    function area(self) -> int {
      self.width * self.height
    }
  }
}

// Out-of-body `implements ... for ...` form.
class Circle {
  radius int
}

implements Shape for Circle {
  function area(self) -> int {
    3 * self.radius * self.radius
  }
}

// Interface in a class-field position.
class ShapeBox {
  shape Shape
}

// ── interface in return position ────────────────────────────────────────

function return_square_as_shape() -> Shape {
  Square { side: 5 }
}

function return_rect_as_shape() -> Shape {
  Rect { width: 3, height: 4 }
}

function return_circle_as_shape() -> Shape {
  Circle { radius: 2 }
}

// ── interface in parameter position (engine-side method dispatch) ───────

function shape_area(s: Shape) -> int {
  s.area()
}

function sum_areas(a: Shape, b: Shape) -> int {
  a.area() + b.area()
}

// ── encode → FFI → decode round trips ───────────────────────────────────

function round_trip_shape(s: Shape) -> Shape {
  s
}

function round_trip_shape_list(shapes: Shape[]) -> Shape[] {
  shapes
}

function round_trip_optional_shape(s: Shape?) -> Shape? {
  s
}

function round_trip_shape_box(b: ShapeBox) -> ShapeBox {
  b
}

function box_area(b: ShapeBox) -> int {
  b.shape.area()
}
