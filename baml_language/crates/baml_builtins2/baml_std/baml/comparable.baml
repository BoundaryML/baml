/// A total natural ordering for values of the implementing type.
///
/// `compare` follows the `sort_by` comparator convention: negative means
/// `self` orders before `other`, zero preserves relative order, positive
/// means `self` orders after `other`.
///
/// `CmpError` is the error a single comparison may throw. A total order that
/// never fails binds `type CmpError = never`; a fallible comparator binds
/// `type CmpError = MyError` and the error propagates to whoever compares (or
/// sorts) the values.
///
/// ```
/// class Resume {
///   name string
///   implements Comparable {
///     type CmpError = never
///     function compare(self, other: Self) -> int throws never {
///       self.name.compare(other.name)
///     }
///   }
/// }
/// ```
///
/// Note: `CmpError` is intentionally *not* defaulted. A defaulted associated
/// type would be injected into a bare `T extends Comparable` bound as the
/// concrete default, which then rejects any implementor that overrides it —
/// so `MyError[].sort()` (a fallible comparator) would stop compiling. Leaving
/// it undefaulted keeps the `Comparable` bound permissive over `CmpError`.
interface Comparable {
  type CmpError

  function compare(self, other: Self) -> int throws CmpError
}

implements Comparable for int {
  type CmpError = never
  function compare(self, other: Self) -> int throws never {
    if (self < other) { -1 } else if (self > other) { 1 } else { 0 }
  }
}

implements Comparable for bigint {
  type CmpError = never
  function compare(self, other: Self) -> int throws never {
    if (self < other) { -1 } else if (self > other) { 1 } else { 0 }
  }
}

implements Comparable for string {
  type CmpError = never
  function compare(self, other: Self) -> int throws never {
    if (self < other) { -1 } else if (self > other) { 1 } else { 0 }
  }
}

/// Sorting for collections whose elements have a natural order.
///
/// Implemented for `T[]` whenever `T implements Comparable`. `SortError` is
/// the element's `CmpError`: sorting a primitive array (`int[]`, `bigint[]`,
/// `string[]`, `float[]`) can never fail, and sorting a user type with a
/// fallible comparator propagates that comparator's error.
interface Sortable {
  type SortError

  function sort(self) -> Self throws SortError
}

implements<T extends Comparable> Sortable for T[] {
  type SortError = T.CmpError

  /// Sorts this array in place by the elements' natural order
  /// (`Comparable.compare`), then returns `self`. The sort is stable. If a
  /// comparison throws mid-sort, the array is left in its pre-sort state
  /// (`sort_by` only writes back once the whole sort completes; the primitive
  /// fast path is infallible).
  ///
  /// Dispatch: naturally-sortable primitive element types
  /// (`int`/`bigint`/`string`/`float`, detected by `_is_primitive_array` from
  /// the first element's runtime tag) take the native `_rust_sort` fast path —
  /// an in-place natural sort with a pure-Rust comparator, no per-pair BAML
  /// round trips. Every other `Comparable` element type falls back to
  /// `sort_by` with a `_compare_shim` comparator.
  ///
  /// The fallback goes through `_compare_shim` rather than `a.compare(b)`
  /// directly: `compare` takes two `Self` params, so it cannot be dispatched
  /// through an interface-typed value, and the static dispatch from this
  /// (stdlib) package can only see `Comparable` impls visible here — not
  /// downstream user classes. `_compare_shim` resolves `compare` on the
  /// element's runtime class instead.
  function sort(self) -> Self throws T.CmpError {
    if (root._is_primitive_array(self)) {
      root._rust_sort(self)
    } else {
      self.sort_by((a: T, b: T) -> int throws T.CmpError { root._compare_shim(a, b) })
    }
  }
}

/// Three-way compares two `Comparable` values by their natural order, resolving
/// `Comparable.compare` on the values' runtime class.
///
/// This is the dispatch shim for the `Sortable` blanket `sort` (above):
/// `compare`'s two `Self` parameters make it undispatchable through an
/// interface-typed value (BEP-044), and a static dispatch from this package
/// cannot enumerate `Comparable` impls defined in downstream packages. Both
/// arguments must share a runtime type (`sort` only ever feeds it elements of
/// one array). Internal (the leading underscore is the convention for
/// non-public stdlib shims); prefer calling `a.compare(b)` directly where the
/// concrete type is statically known.
//baml:mut_vm
//baml:may_yield
function _compare_shim<T extends Comparable>(a: T, b: T) -> int throws T.CmpError {
  $rust_function
}

// Floats order by IEEE 754 `totalOrder` (`f64::total_cmp`): a *total* order
// over all doubles that places NaN deterministically
// (`-NaN < -inf < … < -0.0 < +0.0 < … < +inf < +NaN`), so `compare` — and
// therefore `float[].sort()` — never fails. The comparison is delegated to
// the native `_float_total_cmp` so it is bit-exact `total_cmp` (the sign of
// NaN and of zero participate in the order), matching the comparator the
// native `_rust_sort` fast path uses for the float domain.
implements Comparable for float {
  type CmpError = never

  function compare(self, other: Self) -> int throws never {
    root._float_total_cmp(self, other)
  }
}

/// Three-way compares two floats by IEEE 754 `totalOrder` (`f64::total_cmp`).
///
/// Internal shim backing `Comparable for float` (above) and kept bit-exact
/// with the float comparator inside the native `_rust_sort`; not part of the
/// public `Comparable`/`Sortable` surface.
function _float_total_cmp(a: float, b: float) -> int throws never {
  $rust_function
}

/// Whether `arr` is a homogeneous primitive array (`int`/`bigint`/`string`/
/// `float`) that the native `_rust_sort` fast path can natural-sort, decided
/// by the first element's runtime type tag (a `T[]`'s homogeneity plus the
/// `T extends Comparable` bound at the call sites guarantee the first element
/// speaks for the whole array). Empty arrays return `true` — the native sort
/// is a no-op on them. Internal: this is `Sortable.sort`'s dispatch guard. It
/// tests runtime *values* rather than type arguments for the same reason
/// `_compare_shim` exists: a BAML-level `is`-test on a `T`-typed value
/// constant-folds to false, and frame type args are not reliable across every
/// call path into a blanket impl.
//baml:vm
function _is_primitive_array<T extends Comparable>(arr: T[]) -> bool throws never {
  $rust_function
}

/// Sorts a primitive array (`int`/`bigint`/`string`/`float`) in place by its
/// natural order and returns it (the same array object — no copy). Total and
/// infallible for every supported element type — `float` uses
/// `f64::total_cmp`, a total order over all doubles including NaN, so it
/// never throws. Only reached from `Sortable.sort`'s primitive fast path
/// (guarded by `_is_primitive_array`), where the element type is known to be
/// one of these — other element types use `sort_by`.
///
/// `arr` is typed as a bare generic (`A`, not `T[]` with `T extends
/// Comparable`): the native-glue codegen passes generic parameters as raw
/// values, which is what lets the implementation sort the receiver's backing
/// storage in place and return the receiver itself — a `T[]`-typed parameter
/// would be deconstructed to a slice and re-allocated on return. The
/// `may_yield` directive is likewise glue-shaping only (raw receiver + error
/// channel); the comparator is pure Rust and never yields to BAML.
//baml:mut_vm
//baml:may_yield
function _rust_sort<A>(arr: A) -> A throws never {
  $rust_function
}
