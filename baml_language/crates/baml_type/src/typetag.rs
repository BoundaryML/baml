//! Type tags: the runtime identity of a type head.
//!
//! [`TypeTag`] identifies a type *head* — the declaration a nominal reference
//! names — rather than a type: generic instantiations are distinct types
//! sharing one head. One number serves both roles the runtime needs: the tag
//! the `TypeTag` instruction dispatches on and the relocation-stable identity a
//! heap-anchored head orders and hashes by. The primitive constants below are
//! tags in the same space.
//!
//! # Who assigns a tag
//!
//! The compiler never does. A compiled unit refers to a declaration by an
//! object operand ("the type tag of *this* declaration"), and the tag is
//! assigned by whoever produces the image the declaration lives in:
//!
//! - the **linker** gives every declaration of a static image
//!   `CLASS_BASE + its absolute object index` — monotone in image order,
//!   deterministic for a link, and collision-free by construction;
//! - the **grafter** gives every declaration it loads into a live heap, and
//!   the VM every declaration it creates, a fresh value from a process-wide
//!   counter above [`DYNAMIC_BASE`] ([`TypeTag::fresh_dynamic`]).
//!
//! A tag is meaningful only within the image or process that assigned it;
//! nothing persists one or compares one across engines.
//!
//! # Type Tag Assignment
//!
//! - **Primitives** (`0..CLASS_BASE`): fixed tags for built-in kinds
//! - **Declared heads** (`CLASS_BASE..`): assigned at link or at load, above
//!
//! # Usage
//!
//! The `TypeTag` instruction extracts a type identifier from any value,
//! enabling efficient dispatch on union types: a switch over declaration arms
//! dispatches through a perfect-hash table the image's producer solved over
//! the tags it assigned.

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

/// Uint8Array type tag.
pub const UINT8ARRAY: i64 = 12;

/// Bigint type tag.
pub const BIGINT: i64 = 13;

/// Base value for class type tags (classes start at 100).
pub const CLASS_BASE: i64 = 100;

/// Unknown/invalid type tag.
pub const UNKNOWN: i64 = -1;

/// Width of the link-assigned space above [`CLASS_BASE`]: room for `2^47`
/// declarations in one image, comfortably inside `i64` and disjoint both from
/// the primitive tags below and the dynamic range above.
const STATIC_BITS: u32 = 47;

/// First tag handed to a head created or loaded at run time, placed above the
/// entire link-assigned space so a dynamic tag can never collide with a static
/// one. See [`TypeTag::fresh_dynamic`].
pub const DYNAMIC_BASE: i64 = CLASS_BASE + (1 << STATIC_BITS);

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
/// name honest — `type_tags::INT` and a declaration's tag are the same kind of
/// thing and are compared the same way:
///
/// | range                    | heads                                          |
/// |--------------------------|------------------------------------------------|
/// | [`UNKNOWN`] (`-1`)       | no head — an absent or unresolvable value       |
/// | `0..CLASS_BASE`          | primitives ([`INT`], [`STRING`], …) — built in, nothing declares them |
/// | `CLASS_BASE..DYNAMIC_BASE` | declared heads of a static image, assigned by the linker |
/// | `DYNAMIC_BASE..`         | heads created or loaded at run time, from a monotonic counter |
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
    /// The tag of a declared head in a static image: `CLASS_BASE` plus the
    /// declaration's absolute object index. The linker assigns it while laying
    /// the image out; the loader binds a head by inverting it
    /// ([`Self::static_index`]).
    #[must_use]
    #[expect(clippy::cast_possible_wrap, reason = "bounded by STATIC_BITS above")]
    pub const fn of_static_index(index: usize) -> Self {
        // Bounded in the tag's own width: `usize` may be narrower than the
        // static range (wasm32), where `1usize << STATIC_BITS` does not even
        // compile.
        let index = index as u64;
        assert!(
            index < (1u64 << STATIC_BITS),
            "a static image cannot hold 2^47 declarations"
        );
        Self(CLASS_BASE + index as i64)
    }

    /// The absolute object index a static tag encodes, or `None` for a
    /// primitive or a dynamic tag.
    #[must_use]
    #[expect(clippy::cast_sign_loss, reason = "non-negative by the range check")]
    pub const fn static_index(self) -> Option<usize> {
        if self.0 >= CLASS_BASE && self.0 < DYNAMIC_BASE {
            Some((self.0 - CLASS_BASE) as usize)
        } else {
            None
        }
    }

    /// A fresh tag for a head created or loaded at run time, drawn from a
    /// monotonic counter in the reserved range above [`DYNAMIC_BASE`].
    ///
    /// Each creation is its own type under nominal typing, and a declaration a
    /// grafted package loads is generative in the same way (two `Package.
    /// compile`s of one source are two sets of declarations), so nothing about
    /// a runtime head is derived from a name or a structure.
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

    /// Whether this head was minted at run time rather than assigned by the
    /// linker of a static image.
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
