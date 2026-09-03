//! Native math operators (`+ - * / %` and unary `-`) for primitive values,
//! declared as `baml.ops.{Add,Subtract,Multiply,Divide,Remainder,Negate}`.
//!
//! An operator interface is how generic code asks for an operator:
//! `T extends baml.ops.Add<int>` accepts exactly the types that can be added to
//! an `int`, and implementing the interface is how a type gains `+` in the
//! first place. Python spells the same idea `__add__`; TypeScript has no
//! equivalent.
//!
//! `a + b` and `baml.ops.Add.add(a, b)` always agree, whether the operands'
//! types are known statically or only reached through a bound
//! (`T extends Add<...>`). The same holds for every operator here.
//!
//! Result types, shared by every binary op below: `int` with `int` stays `int`;
//! `int` with `bigint` widens exactly to `bigint`; `float` with `int` or
//! `float` is `float`, the `int` widening lossily past 2^53. There is no
//! `float`/`bigint` impl — mixing those two is a type error rather than a lossy
//! conversion.
//!
//! Behaviour the `throws never` signatures do not capture — panics are orthogonal
//! to the `throws` contract (see `bigint.baml`), so these stay `throws never`:
//!   - `int` arithmetic is checked, never wrapping: a result outside the `int`
//!     range raises the catchable `baml.panics.IntegerOverflow`, including
//!     `int.min_value() / -1` and `-int.min_value()`.
//!   - integer (`int` / `bigint`) `/` and `%` by zero raise
//!     `baml.panics.DivisionByZero`.
//!   - a `bigint` result beyond the workspace size cap raises
//!     `baml.panics.AllocFailure`.
//!   - no `float` operation panics. Arithmetic is IEEE-754 throughout: a zero
//!     divisor yields `±inf` from `/` but `NaN` from `%`, and `0.0 / 0.0` is
//!     `NaN`. Only the integer types panic on a zero divisor, because they have
//!     no value for it.

/// The addition operator, `self + rhs`.
///
/// `Rhs` names the right-hand operand type and `Output` the result, so one type
/// can add several others: `int` implements `Add<int>`, `Add<float>` and
/// `Add<bigint>`. This is the shape of Rust's
/// [`ops::Add`](https://doc.rust-lang.org/std/ops/trait.Add.html).
///
/// An implementation may panic for reasons of its own — the `int` impl panics
/// on overflow, for instance. Panics are orthogonal to the `throws` contract,
/// which is why `add` is `throws never`: a caller reaching the operator through
/// a bound like `T extends baml.ops.Add<int>` cannot enumerate what the
/// implementation it gets might raise, and must be ready for a panic it cannot
/// describe. The same holds for every operator interface in `baml.ops`.
interface Add<Rhs extends root.Concrete> requires root.Concrete {
    /// The sum's type. In the primitive impls, `int` with `int` is `int`, any
    /// `bigint` mix is `bigint`, and any `float` mix is `float`.
    type Output extends root.Concrete;
    /// Adds `rhs` to `self`.
    function add(self, rhs: Rhs) -> Self.Output throws never
}

/// The subtraction operator, `self - rhs`.
///
/// The counterpart of `baml.ops.Add`, with the same `Rhs`/`Output` shape as
/// Rust's [`ops::Sub`](https://doc.rust-lang.org/std/ops/trait.Sub.html).
/// Unary negation is a separate interface, `baml.ops.Negate`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Subtract<Rhs extends root.Concrete> requires root.Concrete {
    /// The difference's type. In the primitive impls, `int` with `int` is
    /// `int`, any `bigint` mix is `bigint`, and any `float` mix is `float`.
    type Output extends root.Concrete;
    /// Subtracts `rhs` from `self`.
    function sub(self, rhs: Rhs) -> Self.Output throws never
}

/// The multiplication operator, `self * rhs`.
///
/// The same `Rhs`/`Output` shape as Rust's
/// [`ops::Mul`](https://doc.rust-lang.org/std/ops/trait.Mul.html).
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Multiply<Rhs extends root.Concrete> requires root.Concrete {
    /// The product's type. In the primitive impls, `int` with `int` is `int`,
    /// any `bigint` mix is `bigint`, and any `float` mix is `float`.
    type Output extends root.Concrete;
    /// Multiplies `self` by `rhs`.
    function mul(self, rhs: Rhs) -> Self.Output throws never
}

/// The division operator, `self / rhs`.
///
/// The shape of Rust's
/// [`ops::Div`](https://doc.rust-lang.org/std/ops/trait.Div.html). What
/// division means is left to the implementation: the integer impls truncate
/// toward zero rather than producing a `float` — as in Rust, but unlike
/// Python's or JavaScript's `/` — and panic on a zero divisor, while `float`
/// division is IEEE-754 and yields `±inf` or `NaN` instead. Divide by a
/// `float` to get a `float`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
///
/// # Examples
///
/// ```baml
/// 7 / 2      // 3    (truncates toward zero)
/// -7 / 2     // -3
/// 7.0 / 2    // 3.5  (the int widens to float)
/// ```
interface Divide<Rhs extends root.Concrete> requires root.Concrete {
    /// The quotient's type. In the primitive impls, `int` with `int` is `int`,
    /// any `bigint` mix is `bigint`, and any `float` mix is `float`.
    type Output extends root.Concrete;
    /// Divides `self` by `rhs`.
    function div(self, rhs: Rhs) -> Self.Output throws never
}

/// The remainder operator, `self % rhs`.
///
/// The same `Rhs`/`Output` shape as Rust's
/// [`ops::Rem`](https://doc.rust-lang.org/std/ops/trait.Rem.html). The
/// primitive impls pair with `baml.ops.Divide`: the remainder is that of the
/// truncating division, so it takes the sign of `self`, not of `rhs` —
/// `-7 % 2` is `-1`, not `1`. That is not a modulo, and so does not agree with
/// Python's `%`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Remainder<Rhs extends root.Concrete> requires root.Concrete {
    /// The remainder's type. In the primitive impls, `int` with `int` is `int`,
    /// any `bigint` mix is `bigint`, and any `float` mix is `float`.
    type Output extends root.Concrete;
    /// The remainder of dividing `self` by `rhs`.
    function rem(self, rhs: Rhs) -> Self.Output throws never
}

/// The unary negation operator, `-self`.
///
/// Takes no `Rhs` — it is single dispatch on the one operand — so it
/// corresponds to Rust's
/// [`ops::Neg`](https://doc.rust-lang.org/std/ops/trait.Neg.html) rather than
/// to `baml.ops.Subtract`.
///
/// An implementation may panic, which `throws never` does not rule out; a
/// caller working through a generic bound cannot enumerate the panics and must
/// be ready for one it cannot describe. See `baml.ops.Add`.
interface Negate requires root.Concrete {
    /// The negation's type. Every primitive impl stays within its own type:
    /// `int` to `int`, `float` to `float`, `bigint` to `bigint`.
    type Output extends root.Concrete;
    /// Negates `self`.
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

// ==== operator dispatch drivers ====

/// (internal) Runtime double dispatch for `a + b`: picks the `baml.ops.Add`
/// impl from *both* operands' runtime types — `a`'s selects the implementing
/// type, `b`'s the `Rhs` — and tail-calls its `add`.
///
/// The compiler emits a `__union_<op>` call for a binary operator whose
/// operands' static types are erased to an abstract form (a union, an
/// interface-existential, or a type variable), where the `implement` rule can
/// only be chosen from the operands' *runtime* types — double dispatch a static
/// call cannot express. Primitive operands keep their fast path and a
/// statically-pinned pair calls the impl directly; only the abstract case lands
/// here.
///
/// `unknown` in and out costs nothing: the type checker has already proved
/// every runtime operand pair is implemented — an unresolved dispatch is an
/// internal error, not a user one — and supplies the real operand types, while
/// the caller re-applies the statically-computed result type to what comes
/// back.
///
/// The four drivers below (`__union_sub`, `__union_mul`, `__union_div`,
/// `__union_rem`) and the unary `__union_neg` work exactly the same way, each
/// against its own interface.
///
/// # Panics
///
/// - Whatever the resolved `add` raises — for an `int` pair, for instance,
///   `baml.panics.IntegerOverflow`.
//baml:mut_vm
//baml:may_yield
function __union_add(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}

/// (internal) Runtime double dispatch for `a - b` through `baml.ops.Subtract`.
/// See `baml.ops.__union_add` for why these drivers exist and what the
/// `unknown` parameters and result mean.
//baml:mut_vm
//baml:may_yield
function __union_sub(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}

/// (internal) Runtime double dispatch for `a * b` through `baml.ops.Multiply`.
/// See `baml.ops.__union_add`.
//baml:mut_vm
//baml:may_yield
function __union_mul(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}

/// (internal) Runtime double dispatch for `a / b` through `baml.ops.Divide`.
/// See `baml.ops.__union_add`.
//baml:mut_vm
//baml:may_yield
function __union_div(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}

/// (internal) Runtime double dispatch for `a % b` through `baml.ops.Remainder`.
/// See `baml.ops.__union_add`.
//baml:mut_vm
//baml:may_yield
function __union_rem(a: unknown, b: unknown) -> unknown throws never {
    $rust_function
}

/// (internal) Unary `-a`: single dispatch on `a`'s runtime type through
/// `baml.ops.Negate`.
///
/// `Negate` takes no `Rhs`, so unlike the binary drivers this resolves from one
/// operand only. See `baml.ops.__union_add` for the rest of the contract.
//baml:mut_vm
//baml:may_yield
function __union_neg(a: unknown) -> unknown throws never {
    $rust_function
}
