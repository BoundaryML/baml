/// Sorting for collections whose elements have a natural order.
///
/// Implemented for `T[]` whenever `T` implements `baml.ops.Compare`. That
/// ordering is total and `throws never`, so sorting is infallible: `sort()` has
/// no error channel and needs no associated error type.
///
/// ```
/// class Resume {
///   name string
///   implements baml.ops.Equals {
///     function eq(self, other: Self) -> bool throws never { self.name == other.name }
///   }
///   implements baml.ops.Compare {
///     function cmp(self, other: Self) -> baml.ops.Ordering throws never {
///       self.name.cmp(other.name)
///     }
///   }
/// }
///
/// resumes.sort()
/// ```
interface Sortable {
  function sort(self) -> Self throws never
}

implements<T extends baml.ops.Compare> Sortable for T[] {
  /// Sorts this array in place by the elements' natural order
  /// (`baml.ops.Compare.cmp`), then returns `self`. The sort is stable.
  ///
  /// Dispatch: naturally-sortable primitive element types
  /// (`int`/`bigint`/`string`/`float`, detected by `_is_primitive_array` from
  /// the first element's runtime tag) take the native `_rust_sort` fast path —
  /// an in-place natural sort with a pure-Rust comparator, no per-pair BAML
  /// round trips. Every other `Compare` element type falls back to `sort_by`
  /// with `a.cmp(b)`.
  ///
  /// `a` and `b` are typed `T` — a bounded type variable, not an
  /// interface-typed value — so `a.cmp(b)` lowers to a virtual call that
  /// resolves the impl from the receiver's runtime type. That reaches
  /// `Compare` impls in downstream packages this one cannot see, which is why
  /// the fallback needs no native dispatch shim.
  function sort(self) -> Self throws never {
    if (root._is_primitive_array(self)) {
      root._rust_sort(self)
    } else {
      self.sort_by((a: T, b: T) -> baml.ops.Ordering throws never { a.cmp(b) })
    }
  }
}

/// Whether `arr` is a homogeneous primitive array (`int`/`bigint`/`string`/
/// `float`) that the native `_rust_sort` fast path can natural-sort, decided
/// by the first element's runtime type tag (a `T[]`'s homogeneity plus the
/// `T extends baml.ops.Compare` bound at the call sites guarantee the first
/// element speaks for the whole array). Empty arrays return `true` — the native
/// sort is a no-op on them. Internal: this is `Sortable.sort`'s dispatch guard.
/// It tests runtime *values* rather than type arguments because a BAML-level
/// `is`-test on a `T`-typed value constant-folds to false, and frame type args
/// are not reliable across every call path into a blanket impl.
//baml:vm
function _is_primitive_array<T extends baml.ops.Compare>(arr: T[]) -> bool throws never {
    $rust_function
}

/// Sorts a primitive array (`int`/`bigint`/`string`/`float`) in place by its
/// natural order and returns it (the same array object — no copy). Total and
/// infallible for every supported element type — `float` uses BAML's total
/// float order, which covers NaN, so it never throws. Only reached from
/// `Sortable.sort`'s primitive fast path
/// (guarded by `_is_primitive_array`), where the element type is known to be
/// one of these — other element types use `sort_by`.
///
/// `arr` is typed as a bare generic (`A`, not `T[]` with `T extends
/// baml.ops.Compare`): the native-glue codegen passes generic parameters as raw
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
