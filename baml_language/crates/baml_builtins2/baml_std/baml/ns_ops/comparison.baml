interface Equals {
    function eq(self, other: Self) -> bool throws never
    function neq(self, other: Self) -> bool throws never {
        !self.eq(other)
    }
}

interface Compare requires Equals {
    function lt(self, other: Self) -> bool throws never
    function le(self, other: Self) -> bool throws never {
        self.lt(other) || self.eq(other)
    }
    function gt(self, other: Self) -> bool throws never {
        !self.le(other)
    }
    function ge(self, other: Self) -> bool throws never {
        !self.lt(other)
    }
}

/// The broad `==` / `!=` operator driver.
///
/// Unlike `Equals` (which is only available on types that implement it), `==`
/// works on any pair of compatible types. The compiler emits specialized
/// comparison bytecode or a direct `Equals.eq` call whenever the operand types
/// are statically known; `equals_equals` is the runtime fallback for erased
/// operands (`unknown`, unions, interfaces) and the recursion target for structural
/// comparison. It compares by the operands' concrete runtime types: a type that
/// implements `Equals` uses its `eq`; a class without `Equals` compares
/// structurally (field by field); enums compare by identity; and operands whose
/// concrete runtime types differ are never equal.
//baml:mut_vm
//baml:may_yield
function equals_equals(a: unknown, b: unknown) -> bool throws never {
    $rust_function
}

// Implement for primitives, containers, and `null`.
//
// Note that the compiler usually special-cases these for performance, emitting
// bytecode that directly compares the primitives. These implementations exist
// so that primitives satisfy interface bounds (`T extends Compare`) and can be
// reached via dynamic dispatch when a comparison cannot be specialized.

implement Equals for int {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for int {
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for bigint {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for bigint {
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for float {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for float {
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
    function gt(self, other: Self) -> bool throws never {
        $rust_function
    }
    function ge(self, other: Self) -> bool throws never {
        $rust_function
    }
    function le(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for bool {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for bool {
    function lt(self, other: Self) -> bool throws never {
        !self && other
    }
    function gt(self, other: Self) -> bool throws never {
        self && !other
    }
    // `ge`/`le` inherit the `Compare` defaults (`ge = !lt`, `le = lt || eq`), which are
    // reflexive: `ge(x, x) = !lt(x, x) = true`, `le(x, x) = lt(x, x) || eq(x, x) = true`.
}

implement Equals for null {
    function eq(self, other: Self) -> bool throws never {
        true
    }
    function neq(self, other: Self) -> bool throws never {
        false
    }
}

implement Equals for string {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Compare for string {
    /// In lexicographic order.
    function lt(self, other: Self) -> bool throws never {
        $rust_function
    }
    /// In lexicographic order.
    function gt(self, other: Self) -> bool throws never {
        $rust_function
    }
    /// In lexicographic order.
    function ge(self, other: Self) -> bool throws never {
        $rust_function
    }
    /// In lexicographic order.
    function le(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement Equals for uint8array {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

// Containers implement `Equals` for *all* element types (unbounded), comparing
// element-wise with the broad `==` (`baml.ops.equals_equals`) rather than the
// element `Equals.eq`. This is sound because `==` is total — it always produces a
// result for operands of the same type — and `self`/`other` are the same container
// type here. It also lets `T[]`/`map<string, V>` satisfy `Equals` bounds for any `T`/`V`.
implement<T> Equals for T[] {
    function eq(self, other: Self) -> bool throws never {
        baml.ops.equals_equals(self, other)
    }
}

implement<V> Equals for map<string, V> {
    function eq(self, other: Self) -> bool throws never {
        baml.ops.equals_equals(self, other)
    }
}
