//! Method-dispatch bytecode snapshots.
//!
//! Mirrors the speedtest `classes::method call` and
//! `interfaces::polymorphic dispatch` workloads to pin down exactly how
//! direct method calls vs. interface (dynamic) dispatch lower to bytecode.
//!
//! Key facts these snapshots lock in:
//!   * A direct `v.norm2()` is a plain static `call user.Vec.norm2` with the
//!     receiver passed as `self` — NO `make_bound_method`, no per-call
//!     allocation. (Bound-method objects are only materialized when a method
//!     is used as a first-class value, e.g. passed as a callback.)
//!   * Interface dispatch `s.area()` lowers to a runtime `is_type` chain +
//!     branch to a static `call` per concrete implementor — no vtable, no
//!     allocation, but O(N) type-checks in the number of implementors.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn direct_method_call() {
    let output = baml_test!(
        r#"
        class Vec {
          x: int
          y: int
          function norm2(self) -> int {
            return self.x * self.x + self.y * self.y
          }
        }
        function main() -> int {
          let v = Vec { x: 3, y: 4 };
          return v.norm2()
        }
        "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function Vec.norm2(self: Vec) -> int {
        load_var self
        load_field .x
        load_var self
        load_field .x
        mul_int
        load_var self
        load_field .y
        load_var self
        load_field .y
        mul_int
        add_int
        return
    }

    function main() -> int {
        load_const 3
        load_const 4
        init_instance user.Vec .x, .y
        call user.Vec.norm2
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(25)));
}

#[tokio::test]
async fn polymorphic_interface_dispatch() {
    let output = baml_test!(
        r#"
        interface Shape {
          function area(self) -> int
        }
        class Square {
          side: int
          implements Shape {
            function area(self) -> int { return self.side * self.side }
          }
        }
        class Rect {
          w: int
          h: int
          implements Shape {
            function area(self) -> int { return self.w * self.h }
          }
        }
        function pick(i: int) -> Shape {
          if i % 2 == 0 { return Square { side: i }; };
          return Rect { w: i, h: 3 };
        }
        function area_of(s: Shape) -> int {
          return s.area()
        }
        function main() -> int {
          return area_of(pick(4)) + area_of(pick(3))
        }
        "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function Rect$stream.Shape.area(self: Rect$stream) -> int {
        load_var self
        load_field .0
        load_var self
        load_field .1
        mul_int
        return
    }

    function Rect.Shape.area(self: Rect) -> int {
        load_var self
        load_field .0
        load_var self
        load_field .1
        mul_int
        return
    }

    function Square$stream.Shape.area(self: Square$stream) -> int {
        load_var self
        load_field .0
        load_var self
        load_field .0
        mul_int
        return
    }

    function Square.Shape.area(self: Square) -> int {
        load_var self
        load_field .0
        load_var self
        load_field .0
        mul_int
        return
    }

    function area_of(s: Shape) -> int {
        load_var s
        is_type Square
        pop_jump_if_false L0
        jump L1

      L0:
        load_var s
        call user.Rect.Shape.area
        jump L2

      L1:
        load_var s
        call user.Square.Shape.area

      L2:
        return
    }

    function main() -> int {
        load_const 4
        call user.pick
        call user.area_of
        store_var _1
        load_const 3
        call user.pick
        call user.area_of
        store_var _3
        load_var _1
        load_var _3
        add_int
        return
    }

    function pick(i: int) -> Shape {
        load_var i
        load_const 2
        mod_int
        load_const 0
        cmp_int_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var i
        load_const 3
        init_instance user.Rect .w, .h
        jump L2

      L1:
        load_var i
        init_instance user.Square .side

      L2:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(25)));
}
