// Native math operators (`+ - * / %` and unary `-`) for the numeric primitives
// and string, declared as `baml.ops.{Add,Subtract,Multiply,Divide,Remainder,
// Negate}` and implemented in `crates/bex_vm/src/package_baml/ops_math.rs`.
//
// Like the comparison ops in `comparison.baml`, the compiler special-cases these
// for statically-typed operands (emitting `AddInt` / `AddFloat` / … bytecode);
// the implementations here back the same operations when reached through a
// generic bound (`T extends Add<...>`) and define the canonical semantics the
// bytecode matches.
//
// Behaviour the `throws never` signatures do not capture — panics are orthogonal
// to the `throws` contract (see `bigint.baml`), so these stay `throws never`:
//   - `int` arithmetic wraps on overflow into the i63 range (matching the
//     `AddInt` / `SubInt` bytecode), so it never throws.
//   - integer (`int` / `bigint`) `/` and `%` by zero raise
//     `baml.panics.DivisionByZero` (`//baml:fallible`).
//   - a `bigint` result beyond the workspace size cap raises
//     `baml.panics.AllocFailure`.
//   - `float` follows IEEE-754 throughout, including `/` and `%` by zero, which
//     yield `inf` / `NaN` rather than panicking.

/// `self + rhs`
interface Add<Rhs extends root.Concrete> {
    type Output extends root.Concrete = Self;
    function add(self, rhs: Rhs) -> Output throws never
}

/// `self - rhs`
interface Subtract<Rhs extends root.Concrete> {
    type Output extends root.Concrete = Self;
    function sub(self, rhs: Rhs) -> Output throws never
}

/// `self * rhs`
interface Multiply<Rhs extends root.Concrete> {
    type Output extends root.Concrete = Self;
    function mul(self, rhs: Rhs) -> Output throws never
}

/// `self / rhs`
interface Divide<Rhs extends root.Concrete> {
    type Output extends root.Concrete = Self;
    function div(self, rhs: Rhs) -> Output throws never
}

/// `self % rhs`
interface Remainder<Rhs extends root.Concrete> {
    type Output extends root.Concrete = Self;
    function rem(self, rhs: Rhs) -> Output throws never
}

/// `-self`
interface Negate {
    function neg(self) -> Self throws never
}

// ==== int ====
implement Add<int> for int {
    function add(self, rhs: int) -> int throws never {
        $rust_function
    }
}
implement Subtract<int> for int {
    function sub(self, rhs: int) -> int throws never {
        $rust_function
    }
}
implement Multiply<int> for int {
    function mul(self, rhs: int) -> int throws never {
        $rust_function
    }
}
implement Divide<int> for int {
    //baml:fallible
    function div(self, rhs: int) -> int throws never {
        $rust_function
    }
}
implement Remainder<int> for int {
    //baml:fallible
    function rem(self, rhs: int) -> int throws never {
        $rust_function
    }
}
implement Negate for int {
    function neg(self) -> int throws never {
        $rust_function
    }
}

// ==== float ====
implement Add<float> for float {
    function add(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Subtract<float> for float {
    function sub(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Multiply<float> for float {
    function mul(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Divide<float> for float {
    function div(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Remainder<float> for float {
    function rem(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Negate for float {
    function neg(self) -> float throws never {
        $rust_function
    }
}

implement Add<int> for float {
    function add(self, rhs: int) -> float throws never {
        $rust_function
    }
}
implement Subtract<int> for float {
    function sub(self, rhs: int) -> float throws never {
        $rust_function
    }
}
implement Multiply<int> for float {
    function mul(self, rhs: int) -> float throws never {
        $rust_function
    }
}
implement Divide<int> for float {
    function div(self, rhs: int) -> float throws never {
        $rust_function
    }
}
implement Remainder<int> for float {
    function rem(self, rhs: int) -> float throws never {
        $rust_function
    }
}

implement Add<float> for int {
    type Output = float;
    function add(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Subtract<float> for int {
    type Output = float;
    function sub(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Multiply<float> for int {
    type Output = float;
    function mul(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Divide<float> for int {
    type Output = float;
    function div(self, rhs: float) -> float throws never {
        $rust_function
    }
}
implement Remainder<float> for int {
    type Output = float;
    function rem(self, rhs: float) -> float throws never {
        $rust_function
    }
}

// ==== bigint ====
implement Add<bigint> for bigint {
    function add(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Subtract<bigint> for bigint {
    function sub(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Multiply<bigint> for bigint {
    //baml:fallible
    function mul(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Divide<bigint> for bigint {
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Remainder<bigint> for bigint {
    //baml:mut_vm
    //baml:fallible
    function rem(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Negate for bigint {
    function neg(self) -> bigint throws never {
        $rust_function
    }
}

implement Add<int> for bigint {
    function add(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}
implement Subtract<int> for bigint {
    function sub(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}
implement Multiply<int> for bigint {
    //baml:fallible
    function mul(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}
implement Divide<int> for bigint {
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}
implement Remainder<int> for bigint {
    //baml:mut_vm
    //baml:fallible
    function rem(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement Add<bigint> for int {
    type Output = bigint;
    function add(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Subtract<bigint> for int {
    type Output = bigint;
    function sub(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Multiply<bigint> for int {
    type Output = bigint;
    //baml:fallible
    function mul(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Divide<bigint> for int {
    type Output = bigint;
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}
implement Remainder<bigint> for int {
    type Output = bigint;
    //baml:mut_vm
    //baml:fallible
    function rem(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

// ==== string ====
// implement<T extends root.Concrete> Add<T> for string {
//     /// Stringifies `rhs` and returns the concatenation of `self` and `rhs`.
//     //baml:vm
//     //baml:fallible
//     function add(self, rhs: T) -> string throws never {
//         $rust_function
//     }
// }
// implement Multiply<int> for string {
//     /// Repeats `self` `rhs` times.
//     function mul(self, rhs: int) -> string throws never {
//         $rust_function
//     }
// }

// ==== operator dispatch drivers ====
//
// Internal: the compiler emits a `__union_<op>` call for a binary operator whose
// operands' static types are erased to an abstract form (a union, an
// interface-existential, or a type variable), where the `implement` rule can
// only be chosen from the operands' *runtime* types — double dispatch a static
// call cannot express. Primitive operands keep their fast-path bytecode and a
// statically-pinned pair calls the impl directly; only the abstract case lands
// here.
//
// `unknown` in and out: the type checker has already proved every runtime
// operand pair is implemented (so an unresolved dispatch is an internal error,
// not a user error), and supplies the real operand types; the caller re-applies
// the statically-computed result type to the `unknown` the driver returns.
//baml:mut_vm
//baml:may_yield
function __union_add(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}
//baml:mut_vm
//baml:may_yield
function __union_sub(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}
//baml:mut_vm
//baml:may_yield
function __union_mul(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}
//baml:mut_vm
//baml:may_yield
function __union_div(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}
//baml:mut_vm
//baml:may_yield
function __union_rem(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}
// Unary `-a`: single dispatch on `a`'s runtime type through `baml.ops.Negate`.
//baml:mut_vm
//baml:may_yield
function __union_neg(a: unknown) -> unknown throws never {
    $rust_function
}
