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
    function ge(self, other: Self) -> bool throws never {
        self
    }
    function le(self, other: Self) -> bool throws never {
        !self
    }
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

implement<T extends Equals> Equals for T[] {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}

implement<K extends Equals, V extends Equals> Equals for map<K, V> {
    function eq(self, other: Self) -> bool throws never {
        $rust_function
    }
}
