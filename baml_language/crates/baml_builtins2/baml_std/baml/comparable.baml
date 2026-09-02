/// A total natural ordering for values of the implementing type.
///
/// `compare` follows the `sort_by` comparator convention: negative orders
/// `self` before `other`, zero preserves their relative order, positive orders
/// `self` after `other`.
///
/// `CompareError` is the error a single comparison may throw. An order that
/// never fails binds `type CompareError = never`; a fallible comparator binds
/// its own error type, which propagates to whoever compares or sorts the
/// values.
///
/// ```baml
/// class Resume {
///   name string
///   implements Comparable {
///     type CompareError = never
///     function compare(self, other: Self) -> int throws never {
///       self.name.compare(other.name)
///     }
///   }
/// }
/// ```
///
/// `CompareError` is deliberately not defaulted, so every implementor states
/// its error type outright. Defaulting it would be sound — a bare
/// `T extends Comparable` bound pins nothing, so an implementor that overrides
/// the default still satisfies the bound and its error type flows through — but
/// a comparator's failure mode is worth writing down.
interface Comparable {
  type CompareError

  function compare(self, other: Self) -> int throws Self.CompareError
}

implements Comparable for int {
  type CompareError = never
  function compare(self, other: Self) -> int throws never {
    if (self < other) { -1 } else if (self > other) { 1 } else { 0 }
  }
}

implements Comparable for bigint {
  type CompareError = never
  function compare(self, other: Self) -> int throws never {
    if (self < other) { -1 } else if (self > other) { 1 } else { 0 }
  }
}

implements Comparable for string {
  type CompareError = never
  function compare(self, other: Self) -> int throws never {
    if (self < other) { -1 } else if (self > other) { 1 } else { 0 }
  }
}

/// Sorting for collections whose elements have a natural order.
///
/// Implemented for `T[]` whenever `T implements Comparable`. `SortError` is the
/// element's `CompareError`, so sorting a primitive array (`int[]`, `bigint[]`,
/// `string[]`, `float[]`) can never fail, while sorting a user type propagates
/// its comparator's error.
interface Sortable {
  type SortError

  function sort(self) -> Self throws Self.SortError
}

implements<T extends Comparable> Sortable for T[] {
  type SortError = T.CompareError

  /// Sorts this array in place by the elements' natural order
  /// (`Comparable.compare`) and returns `self`. The sort is stable. A
  /// comparison that throws mid-sort leaves the array in its pre-sort state:
  /// `sort_by` writes back only once the whole sort completes, and the
  /// primitive fast path is infallible.
  ///
  /// Arrays of `int`, `bigint`, `string`, or `float` take a native fast path
  /// that sorts in place with no per-pair call back into BAML. Every other
  /// `Comparable` element type sorts through `sort_by`, resolving `compare` on
  /// the element's runtime class, so an implementation declared in any package
  /// is honored.
  function sort(self) -> Self throws T.CompareError {
    if (root._is_primitive_array(self)) {
      root._rust_sort(self)
    } else {
      self.sort_by((a: T, b: T) -> int throws T.CompareError { root._compare_shim(a, b) })
    }
  }
}

/// (internal) Three-way compares two `Comparable` values by their natural
/// order, resolving `Comparable.compare` on the values' runtime class.
///
/// The dispatch shim for the `Sortable` blanket `sort` above: `compare`'s two
/// `Self` parameters make it undispatchable through an interface-typed value
/// (BEP-044), and a static dispatch from this package cannot enumerate
/// `Comparable` impls defined in downstream packages. Both arguments must share
/// a runtime type, which `sort` guarantees by only ever feeding it elements of
/// one array. Prefer calling `a.compare(b)` directly where the concrete type is
/// statically known.
//baml:mut_vm
//baml:may_yield
function _compare_shim<T extends Comparable>(a: T, b: T) -> int throws T.CompareError {
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
  type CompareError = never

  function compare(self, other: Self) -> int throws never {
    root._float_total_cmp(self, other)
  }
}

/// (internal) Three-way compares two floats by IEEE 754 `totalOrder`.
///
/// Backs `Comparable for float` above and is kept bit-exact with the float
/// comparator inside the native `_rust_sort`. Not part of the public
/// `Comparable` / `Sortable` surface.
function _float_total_cmp(a: float, b: float) -> int throws never {
    $rust_function
}

/// (internal) Whether `arr` is a homogeneous primitive array (`int`, `bigint`,
/// `string`, `float`) that the native `_rust_sort` fast path can natural-sort.
///
/// Decided by the first element's runtime type tag: a `T[]`'s homogeneity plus
/// the `T extends Comparable` bound at the call sites let the first element
/// speak for the whole array. Empty arrays return `true` — the native sort is a
/// no-op on them.
///
/// This is `Sortable.sort`'s dispatch guard. It tests runtime *values* rather
/// than type arguments for the same reason `_compare_shim` exists: a BAML-level
/// `is`-test on a `T`-typed value constant-folds to false, and frame type args
/// are not reliable across every call path into a blanket impl.
//baml:vm
function _is_primitive_array<T extends Comparable>(arr: T[]) -> bool throws never {
    $rust_function
}

/// (internal) Sorts a primitive array (`int`, `bigint`, `string`, `float`) in
/// place by its natural order and returns the same array object, with no copy.
///
/// Total and infallible for every supported element type: `float` uses IEEE
/// 754 `totalOrder`, a total order over all doubles including NaN, so it never
/// throws. Only reached from `Sortable.sort`'s primitive fast path (guarded by
/// `_is_primitive_array`), where the element type is known to be one of these;
/// every other element type goes through `sort_by`.
///
/// `arr` is typed as a bare generic (`A`, not `T[]` with `T extends
/// Comparable`) because the native-glue codegen passes generic parameters as
/// raw values, which is what lets the implementation sort the receiver's
/// backing storage in place and return the receiver itself — a `T[]`-typed
/// parameter would be deconstructed and re-allocated on return. The `may_yield`
/// directive is likewise glue-shaping only (raw receiver plus error channel);
/// the comparator is pure Rust and never yields to BAML.
//baml:mut_vm
//baml:may_yield
function _rust_sort<A>(arr: A) -> A throws never {
    $rust_function
}
