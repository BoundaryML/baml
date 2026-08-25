//! Type tag constants for BAML runtime type identification.
//!
//! This crate defines global type tag constants used by both the compiler
//! (for generating type-discriminated switch statements) and the VM
//! (for runtime type identification).
//!
//! [`TypeTag`] is the typed form of that space. It identifies a type *head* —
//! the declaration a nominal reference names — rather than a type: generic
//! instantiations are distinct types sharing one head. It doubles as the tag
//! baked into bytecode and as the stable key a heap-anchored head orders and
//! hashes by, deliberately one number since two would have to be kept in
//! agreement. The primitive constants below are tags in the same space.
//!
//! # Type Tag Assignment
//!
//! - **Primitives** (0-99): Fixed tags for built-in types
//! - **Classes** (100+): Dynamically assigned at compile time as `CLASS_BASE + index`
//!
//! # Usage
//!
//! The `TypeTag` instruction extracts a type identifier from any value,
//! enabling efficient jump table dispatch on union types.
//!
//! # Limitations: Class Type Tags and Jump Tables
//!
//! Class type tags are **globally assigned** in declaration order. When a
//! `match` operates on a union of a few classes out of many, the tags may be
//! sparse across the range. The emitter's jump table strategy requires ≥50%
//! density, so in projects with many classes it will typically fall back to
//! sequential `instanceof` chains — making the jump table path for class
//! matching effectively dead code.
//!
//! This matches how other languages handle it: Rust/Haskell use dense
//! per-enum discriminants for ADTs (always jump-table friendly), while
//! Java/Kotlin/C# always use `instanceof` chains for class type matching.
//!
//! A proper fix would be **tagged union wrapping**: treat `Cat | Dog | Bird`
//! as a runtime type that wraps its payload with a union-local discriminant
//! `0..N-1`, assigned at the point a value enters the union. This would make
//! class matching O(1) via dense jump tables, but requires the runtime to
//! distinguish union types from their constituents.

use borsh::{BorshDeserialize, BorshSerialize};

/// Integer type tag.
pub const INT: i64 = 0;

/// String type tag.
pub const STRING: i64 = 1;

/// Boolean type tag.
pub const BOOL: i64 = 2;

/// Null type tag.
pub const NULL: i64 = 3;

/// Float type tag.
pub const FLOAT: i64 = 4;

/// Enum variant type tag (all variants share this).
pub const ENUM: i64 = 5;

/// List/array type tag.
pub const LIST: i64 = 6;

/// Map type tag.
pub const MAP: i64 = 7;

/// Function type tag.
pub const FUNCTION: i64 = 8;

/// Future type tag.
pub const FUTURE: i64 = 9;

/// `Type` meta-type tag.
pub const TYPE: i64 = 10;

/// `Collector` type tag.
pub const COLLECTOR: i64 = 11;

/// Uint8Array type tag.
pub const UINT8ARRAY: i64 = 12;

/// Bigint type tag.
pub const BIGINT: i64 = 13;

/// Base value for class type tags (classes start at 100).
pub const CLASS_BASE: i64 = 100;

/// Unknown/invalid type tag.
pub const UNKNOWN: i64 = -1;

/// Width of the content-addressed hash space above [`CLASS_BASE`]. Keeps a
/// statically-derived tag comfortably inside `i64` and disjoint both from the
/// primitive tags below and the dynamic range above.
const HASH_BITS: u32 = 47;

/// First tag handed to a *runtime-created* head, placed above the entire
/// content-addressed space so a dynamic tag can never collide with a
/// statically-derived one. See [`TypeTag::fresh_dynamic`].
pub const DYNAMIC_BASE: i64 = CLASS_BASE + (1 << HASH_BITS);

/// A type *head*: the thing a nominal reference names, and the identity the
/// `TypeTag` instruction dispatches on.
///
/// Deliberately not "type id" — a head is coarser than a type. Generic
/// instantiations are distinct types that share one head (`Box<int>` and
/// `Box<string>` are both `Box`), so a tag identifies the declaration, never the
/// applied type. The arguments live in the surrounding `Ty`, not here.
///
/// # One space
///
/// This is the whole tag space, primitives included, which is what makes the
/// name honest — `type_tags::INT` and a class's tag are the same kind of thing
/// and are compared the same way:
///
/// | range                    | heads                                          |
/// |--------------------------|------------------------------------------------|
/// | [`UNKNOWN`] (`-1`)       | no head — an absent or unresolvable value       |
/// | `0..CLASS_BASE`          | primitives ([`INT`], [`STRING`], …) — built in, nothing declares them |
/// | `CLASS_BASE..DYNAMIC_BASE` | declared heads, content-addressed from the fully-qualified name |
/// | `DYNAMIC_BASE..`         | runtime-created heads, from a monotonic counter |
///
/// Only the last two are *declared* heads that a nominal type reference can
/// point at; [`is_head`](Self::is_head) draws that line.
///
/// One number serves both roles the runtime needs — the tag baked into bytecode
/// and the relocation-stable identity a heap-anchored head orders and hashes by.
/// They must agree, so they are the same value rather than parallel concepts.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, BorshSerialize, BorshDeserialize,
)]
pub struct TypeTag(i64);

impl TypeTag {
    /// The content-addressed tag of a declared head:
    /// `CLASS_BASE + (fnv1a64(fq_name) & 47 bits)`.
    ///
    /// A head's tag depends only on its fully-qualified name — never on what
    /// else exists or the order files compile in. That stability is what lets
    /// compiled bytecode be cached and relinked per file (tags baked into
    /// match-dispatch tables, comparison chains, and constant pools never shift
    /// when unrelated code changes) and gives dynamically loaded declarations
    /// order-independent tags. It also makes the tag stable across *runs*, which
    /// a counter could not offer.
    ///
    /// The hash is a hand-inlined FNV-1a 64 so the value is pinned by this file
    /// alone — no dependency version can silently re-tag every head. Collisions
    /// (47-bit space) are detected at emit time and reported as compile errors,
    /// which is why `fq_name` must be unique across *all* declaration kinds —
    /// class, enum, interface, alias — not merely within one.
    #[must_use]
    pub fn of_head(fq_name: &str) -> Self {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h = FNV_OFFSET;
        for byte in fq_name.as_bytes() {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
        #[allow(clippy::cast_possible_wrap)]
        {
            Self(CLASS_BASE + (h & ((1 << HASH_BITS) - 1)) as i64)
        }
    }

    /// A fresh tag for a head created at runtime, drawn from a monotonic counter
    /// in the reserved range above [`DYNAMIC_BASE`].
    ///
    /// Runtime-created heads are deliberately *not* content-addressed. Two
    /// reasons: they may have no globally-namespaced name to address by, and
    /// hashing a structural description would silently give two separately
    /// created declarations the same identity — wrong under nominal typing,
    /// where each creation is its own type.
    ///
    /// Tags are never recycled. A collected head's tag simply goes unused, which
    /// costs nothing: the space is sparse by construction and nothing enumerates
    /// it. Reuse would be safe anyway — a head is only collected once
    /// unreachable, so nothing can still refer to it — but a counter avoids
    /// needing that argument at all.
    #[must_use]
    pub fn fresh_dynamic() -> Self {
        use std::sync::atomic::{AtomicI64, Ordering};
        // One counter per process rather than per VM, so a tag is unambiguous
        // even if a head is ever observed from another VM in the same process.
        static NEXT: AtomicI64 = AtomicI64::new(DYNAMIC_BASE);
        let tag = NEXT.fetch_add(1, Ordering::Relaxed);
        // Exhaustion is not a practical concern, so if you are reading this
        // wondering whether it needs handling: it does not. The range is
        // everything above `DYNAMIC_BASE`, i.e. 2^63 - 2^47 ≈ 9.2e18 tags. Even
        // counting *cumulative* creations rather than live heads — tags are
        // never recycled, so churn burns the space at the creation rate — a
        // sustained million per second would take ~292,000 years to run out.
        //
        // The check must read `fetch_add`'s *return* value, not the counter:
        // atomic add wraps rather than panicking, even in debug. So the call
        // handing out `i64::MAX` succeeds (a valid, unique tag) and leaves the
        // counter at `i64::MIN`; the next call trips this and dies. The failure
        // mode is a panic on the first unsafe tag, never a silent duplicate.
        assert!(
            tag >= DYNAMIC_BASE,
            "runtime type tag space exhausted (wrapped past i64)",
        );
        Self(tag)
    }

    /// The integer this tag is spelled as in bytecode — the value the `TypeTag`
    /// instruction compares against, and the form the primitive constants in
    /// this module are written in.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// Adopt a raw tag, such as a primitive constant from this module or a value
    /// decoded from bytecode.
    ///
    /// Total, because every `i64` the runtime produces as a tag is one — the
    /// primitives, [`UNKNOWN`], and both head ranges all live here. Use
    /// [`is_head`](Self::is_head) to ask whether a tag names something declared.
    #[must_use]
    pub const fn from_i64(tag: i64) -> Self {
        Self(tag)
    }

    /// Whether this tag names a *declared* head — a class, enum, interface, or
    /// alias — as opposed to a primitive or [`UNKNOWN`].
    ///
    /// Only a declared head can be the target of a nominal type reference, so
    /// this is the predicate that separates the two halves of the space.
    #[must_use]
    pub const fn is_head(self) -> bool {
        self.0 >= CLASS_BASE
    }

    /// Whether this head was minted at runtime rather than derived from a name.
    #[must_use]
    pub const fn is_dynamic(self) -> bool {
        self.0 >= DYNAMIC_BASE
    }
}

impl std::fmt::Display for TypeTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Content-addressed class type tag.
///
/// Retained as the spelling used where a bare bytecode `i64` is wanted; the
/// tag itself is [`TypeTag::of_head`].
#[must_use]
pub fn class_type_tag(fq_name: &str) -> i64 {
    TypeTag::of_head(fq_name).as_i64()
}
