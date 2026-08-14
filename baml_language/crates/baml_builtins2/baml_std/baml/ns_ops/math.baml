// Native math operators (`+ - * / %` and unary `-`) for primitive values,
// declared as `baml.ops.{Add,Subtract,Multiply,Divide,Remainder,Negate}` and
// implemented in `crates/bex_vm/src/package_baml/ops_math.rs`.
//
// Like the comparison ops in `comparison.baml`, the compiler special-cases these
// for statically-typed operands (emitting `AddInt` / `AddFloat` / … bytecode);
// the implementations here back the same operations when reached through a
// generic bound (`T extends Add<...>`) and define the canonical semantics the
// bytecode matches.
//
// Behaviour the `throws never` signatures do not capture — panics are orthogonal
// to the `throws` contract (see `bigint.baml`), so these stay `throws never`.
// Every case matches the opcodes exactly (B-266: checked, not wrapping):
//   - `int` arithmetic raises catchable `baml.panics.IntegerOverflow` on a
//     result outside the i63 range (incl. `int.min_value() / -1` and
//     `-int.min_value()`), like the `AddInt` / `SubInt` / `MulInt` bytecode.
//   - integer (`int` / `bigint`) `/` and `%` by zero raise
//     `baml.panics.DivisionByZero` (`//baml:fallible`).
//   - a `bigint` result beyond the workspace size cap raises
//     `baml.panics.AllocFailure`.
//   - `float` `+ - * %` follow IEEE-754 (`%` by zero yields `NaN`); `/` by zero
//     raises `baml.panics.DivisionByZero`, like the `DivFloat` bytecode.

/// `self + rhs`
interface Add<Rhs extends root.Concrete> requires root.Concrete {
    type Output extends root.Concrete;
    /// The `+` operator.
    function add(self, rhs: Rhs) -> Self.Output throws never
}

/// `self - rhs`
interface Subtract<Rhs extends root.Concrete> requires root.Concrete {
    type Output extends root.Concrete;
    /// The `-` operator.
    function sub(self, rhs: Rhs) -> Self.Output throws never
}

/// `self * rhs`
interface Multiply<Rhs extends root.Concrete> requires root.Concrete {
    type Output extends root.Concrete;
    /// The `*` operator.
    function mul(self, rhs: Rhs) -> Self.Output throws never
}

/// `self / rhs`
interface Divide<Rhs extends root.Concrete> requires root.Concrete {
    type Output extends root.Concrete;
    /// The `/` operator.
    function div(self, rhs: Rhs) -> Self.Output throws never
}

/// `self % rhs`
interface Remainder<Rhs extends root.Concrete> requires root.Concrete {
    type Output extends root.Concrete;
    /// The `%` operator.
    function rem(self, rhs: Rhs) -> Self.Output throws never
}

/// `-self`
interface Negate requires root.Concrete {
    type Output extends root.Concrete;
    /// The unary `-` operator.
    function neg(self) -> Self.Output throws never
}

// ==== int ====
implement Add<int> for int {
    type Output = int;
    //baml:fallible
    function add(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement Subtract<int> for int {
    type Output = int;
    //baml:fallible
    function sub(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement Multiply<int> for int {
    type Output = int;
    //baml:fallible
    function mul(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement Divide<int> for int {
    type Output = int;
    //baml:fallible
    function div(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement Remainder<int> for int {
    type Output = int;
    //baml:fallible
    function rem(self, rhs: int) -> int throws never {
        $rust_function
    }
}

implement Negate for int {
    type Output = int;
    //baml:fallible
    function neg(self) -> int throws never {
        $rust_function
    }
}

// ==== float ====
implement Add<float> for float {
    type Output = float;
    function add(self, rhs: float) -> float throws never {
        $rust_function
    }
}

implement Subtract<float> for float {
    type Output = float;
    function sub(self, rhs: float) -> float throws never {
        $rust_function
    }
}

implement Multiply<float> for float {
    type Output = float;
    function mul(self, rhs: float) -> float throws never {
        $rust_function
    }
}

implement Divide<float> for float {
    type Output = float;
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: float) -> float throws never {
        $rust_function
    }
}

implement Remainder<float> for float {
    type Output = float;
    function rem(self, rhs: float) -> float throws never {
        $rust_function
    }
}

implement Negate for float {
    type Output = float;
    function neg(self) -> float throws never {
        $rust_function
    }
}

implement Add<int> for float {
    type Output = float;
    function add(self, rhs: int) -> float throws never {
        $rust_function
    }
}

implement Subtract<int> for float {
    type Output = float;
    function sub(self, rhs: int) -> float throws never {
        $rust_function
    }
}

implement Multiply<int> for float {
    type Output = float;
    function mul(self, rhs: int) -> float throws never {
        $rust_function
    }
}

implement Divide<int> for float {
    type Output = float;
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: int) -> float throws never {
        $rust_function
    }
}

implement Remainder<int> for float {
    type Output = float;
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
    //baml:mut_vm
    //baml:fallible
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
    type Output = bigint;
    function add(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement Subtract<bigint> for bigint {
    type Output = bigint;
    function sub(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement Multiply<bigint> for bigint {
    type Output = bigint;
    //baml:fallible
    function mul(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement Divide<bigint> for bigint {
    type Output = bigint;
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement Remainder<bigint> for bigint {
    type Output = bigint;
    //baml:mut_vm
    //baml:fallible
    function rem(self, rhs: bigint) -> bigint throws never {
        $rust_function
    }
}

implement Negate for bigint {
    type Output = bigint;
    function neg(self) -> bigint throws never {
        $rust_function
    }
}

implement Add<int> for bigint {
    type Output = bigint;
    function add(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement Subtract<int> for bigint {
    type Output = bigint;
    function sub(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement Multiply<int> for bigint {
    type Output = bigint;
    //baml:fallible
    function mul(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement Divide<int> for bigint {
    type Output = bigint;
    //baml:mut_vm
    //baml:fallible
    function div(self, rhs: int) -> bigint throws never {
        $rust_function
    }
}

implement Remainder<int> for bigint {
    type Output = bigint;
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

implement Add<string> for string {
    type Output = string;
    function add(self, rhs: string) -> string throws never {
        $rust_function
    }
}

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
