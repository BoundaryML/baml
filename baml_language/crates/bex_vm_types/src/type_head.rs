//! The runtime's nominal type head.
//!
//! [`TypeHead`] is what the runtime substitutes for `TypeName` as the `N`
//! parameter of the `baml_type` family, giving `RealizedTy<TypeHead>` /
//! `RuntimeTy<TypeHead>`. A head names a type by pointing directly at the
//! `Object::Class` / `Object::Enum` / `Object::Interface` that defines it,
//! instead of by a package-qualified string that must be looked up.

use baml_type::typetag::TypeTag;

use crate::HeapPtr;

/// A nominal type reference: a [`TypeTag`] identity, plus a pointer to reach the
/// declaration it names.
///
/// Names a *head*, not a type — generic instantiations are distinct types that
/// share one head, and their arguments live in the surrounding `Ty`.
///
/// # The tag is the identity; the pointer is an access path
///
/// **[`TypeTag`] answers "which head is this?"** Every relation — `Eq`, `Ord`,
/// `Hash` — keys on it and nothing else, so identity is a plain integer compare
/// that never touches the heap or depends on collector state. Tags are unique
/// per declaration by construction: named heads are content-addressed and their
/// collisions are rejected at emit time, and runtime-created heads come from a
/// monotonic counter that never repeats. Two heads bearing the same tag *are*
/// the same head.
///
/// **[`HeapPtr`] answers "where is its declaration?"** It is how a field list,
/// variant set, or method table is reached — a dereference instead of a
/// package-map lookup, and the only way to reach a runtime-created declaration
/// at all, since it has no globally-namespaced name to look up. It is
/// deliberately *not* part of identity.
///
/// Keeping the pointer out of identity is what makes the relations mutually
/// consistent, which `Ord` requires: `cmp` returns `Equal` exactly when `eq`
/// returns `true`, and equal heads hash equally. Splitting them — identity by
/// address, ordering by tag — would let two values compare `Equal` while being
/// unequal, quietly corrupting `sort`, `dedup`, and any `BTreeMap` keyed on a
/// type.
///
/// It also decouples comparison from the collector. **The collector relocates
/// objects**, so a pointer's numeric value is not stable over time; anything
/// keyed on it would shift underneath a collection. That matters concretely: the
/// type algebra sorts union members into canonical form by `Ord`, and canonical
/// form must not change under a GC, nor may a long-lived memo rehash itself. A
/// tag is assigned once and travels with the declaration, so comparing two heads
/// is correct whether or not either pointer has been forwarded yet. Only
/// *dereferencing* requires an up-to-date pointer.
///
/// # Serialization
///
/// Deliberately not `Borsh`. A head is a live pointer into this process's heap
/// and can have no meaning in a file or across an FFI boundary, so
/// `RealizedTy<TypeHead>` is *statically* unserializable — a compile error at
/// the boundary rather than a runtime failure. Anything crossing a boundary
/// converts to a name-headed type first.
///
/// # GC obligation
///
/// The pointer makes every reachable `TypeHead` a GC edge: it keeps the
/// declaration alive, must be traced, and must be forwarded after a move. Types
/// reach heads through arbitrary nesting, so the walk is generated rather than
/// hand-written — see `visit_heads` / `visit_heads_mut` on every family member.
///
/// A missed head is a dangling pointer, but note what it is *not*: identity
/// keeps working, because comparison never reads the pointer. So the failure
/// surfaces on the next dereference, not as types mysteriously comparing
/// unequal.
#[derive(Clone, Copy, Debug)]
pub struct TypeHead {
    ptr: HeapPtr,
    tag: TypeTag,
}

impl TypeHead {
    /// Bind a tag to the object declaring it.
    ///
    /// `ptr` must point at the `Object::Class`/`Object::Enum`/`Object::Interface`
    /// that `tag` identifies. Nothing checks the pairing, and comparison never
    /// consults `ptr`, so a mismatched pair produces a head that compares as one
    /// declaration but dereferences to another.
    #[must_use]
    pub fn new(ptr: HeapPtr, tag: TypeTag) -> Self {
        debug_assert!(
            tag.is_head(),
            "a type head must carry a declared head's tag, not a primitive",
        );
        Self { ptr, tag }
    }

    /// Where the declaration lives — for dereferencing it, and for the collector
    /// to trace. Not an identity; compare [`tag`](Self::tag) instead.
    #[must_use]
    pub fn ptr(self) -> HeapPtr {
        self.ptr
    }

    /// This head's identity.
    #[must_use]
    pub fn tag(self) -> TypeTag {
        self.tag
    }

    /// Repoint this head after the collector has moved its declaration.
    ///
    /// Identity is untouched: a moved head remains equal to, and hashes and
    /// orders with, every other reference to the same declaration.
    pub fn forward_to(&mut self, moved: HeapPtr) {
        self.ptr = moved;
    }
}

// All three relations key on `tag` alone — hand-written rather than derived,
// since deriving would fold `ptr` in and make identity address-dependent.

impl PartialEq for TypeHead {
    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag
    }
}

impl Eq for TypeHead {}

impl PartialOrd for TypeHead {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TypeHead {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tag.cmp(&other.tag)
    }
}

impl std::hash::Hash for TypeHead {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tag.hash(state);
    }
}

#[cfg(test)]
#[expect(
    unsafe_code,
    reason = "a head must be built from a real pointer to exercise its relations; \
              these point at stack slots that outlive them and are never dereferenced"
)]
mod tests {
    use super::*;

    fn head(slot: &mut crate::Object, tag: i64) -> TypeHead {
        // SAFETY: `slot` outlives the head in each test below, so the pointer
        // stays valid; these tests exercise the relations, never a deref.
        // Under `heap_debug` the epoch is inert here — these pointers never
        // reach a real heap — so 0, as the other reconstruction paths pass.
        let raw = std::ptr::from_mut(slot);
        #[cfg(feature = "heap_debug")]
        let ptr = unsafe { HeapPtr::from_ptr(raw, 0) };
        #[cfg(not(feature = "heap_debug"))]
        let ptr = unsafe { HeapPtr::from_ptr(raw) };
        TypeHead::new(ptr, TypeTag::from_i64(tag))
    }

    fn slot() -> crate::Object {
        crate::Object::String(bex_str::BexStr::empty())
    }

    /// Identity is the tag, not the address — so the same declaration reached
    /// through two different pointers is one head, and `Eq`/`Ord` agree as the
    /// `Ord` contract requires. Pins this against a future `derive`, which would
    /// fold `ptr` back in.
    #[test]
    fn identity_is_the_tag_not_the_address() {
        let (mut a, mut b) = (slot(), slot());
        let first = head(&mut a, 100);
        let second = head(&mut b, 101);

        assert_ne!(first, second);
        assert!(first < second);

        // Distinct addresses, same tag: the same head. A stale pointer and a
        // forwarded one must not look like two types.
        let same_tag_elsewhere = head(&mut b, 100);
        assert_eq!(first, same_tag_elsewhere);
        assert_eq!(first.cmp(&same_tag_elsewhere), std::cmp::Ordering::Equal);

        // `Ord` and `Eq` must not disagree, or `sort`/`dedup`/`BTreeMap` break.
        for (x, y) in [(first, second), (first, same_tag_elsewhere)] {
            assert_eq!(
                x.cmp(&y) == std::cmp::Ordering::Equal,
                x == y,
                "cmp must report Equal exactly when eq does",
            );
        }
    }

    /// The property the collector depends on: moving a declaration changes its
    /// address but nothing observable. Comparison never consults the pointer, so
    /// a head is correct whether or not it has been forwarded yet — only
    /// dereferencing needs the new address.
    #[test]
    fn forwarding_leaves_identity_untouched() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        let hash_of = |h: &TypeHead| {
            let mut hasher = DefaultHasher::new();
            h.hash(&mut hasher);
            hasher.finish()
        };

        let (mut from_space, mut to_space, mut other) = (slot(), slot(), slot());
        let before = head(&mut from_space, 500);
        let ordering_peer = head(&mut other, 900);
        let (hash_before, was_less) = (hash_of(&before), before < ordering_peer);

        let mut after = before;
        // SAFETY: `to_space` outlives `after`; no deref occurs.
        let raw = std::ptr::from_mut(&mut to_space);
        #[cfg(feature = "heap_debug")]
        let moved = unsafe { HeapPtr::from_ptr(raw, 0) };
        #[cfg(not(feature = "heap_debug"))]
        let moved = unsafe { HeapPtr::from_ptr(raw) };
        after.forward_to(moved);

        assert_eq!(after, before, "a moved head is still the same head");
        assert_eq!(after.tag(), before.tag());
        assert_eq!(hash_of(&after), hash_before, "hash must survive a move");
        assert_eq!(after < ordering_peer, was_less, "order must survive a move");
        assert_ne!(
            after.ptr(),
            before.ptr(),
            "only the access path changed, and it is not identity",
        );
    }

    /// Dynamic tags are disjoint from every content-addressed one, so a
    /// runtime-created head can never be confused with a compiled declaration.
    /// Primitives sit below both, and are not heads at all.
    #[test]
    fn dynamic_tags_never_collide_with_named_ones() {
        let dynamic = TypeTag::fresh_dynamic();
        assert!(dynamic.is_dynamic() && dynamic.is_head());
        assert!(!TypeTag::of_head("baml.llm.PromptAst").is_dynamic());
        assert!(dynamic.as_i64() > TypeTag::of_head("x").as_i64());
        assert_ne!(TypeTag::fresh_dynamic(), dynamic, "tags are never reused");

        // The primitives share the space but are not declared heads.
        for primitive in [
            baml_type::typetag::INT,
            baml_type::typetag::STRING,
            baml_type::typetag::UNKNOWN,
        ] {
            assert!(!TypeTag::from_i64(primitive).is_head());
        }
    }

    /// The head substitutes into the `baml_type` family, which is the point of
    /// the whole exercise: the runtime's types are the *same* types the compiler
    /// uses, at a different head, not a parallel hierarchy.
    ///
    /// Instantiating the family here is also what forces the per-monomorphization
    /// layout asserts guarding the reinterpreting conversions to be evaluated at
    /// `TypeHead` — they are inline `const` blocks in generic functions, so they
    /// fire only where the instantiation actually exists.
    #[test]
    fn family_instantiates_at_a_heap_head() {
        use baml_type::{RealizedTy, TyAttr};

        let mut definition = slot();
        // SAFETY: `definition` outlives every head below; nothing dereferences it.
        let raw = std::ptr::from_mut(&mut definition);
        #[cfg(feature = "heap_debug")]
        let ptr = unsafe { HeapPtr::from_ptr(raw, 0) };
        #[cfg(not(feature = "heap_debug"))]
        let ptr = unsafe { HeapPtr::from_ptr(raw) };
        let head = TypeHead::new(ptr, TypeTag::of_head("demo.Person"));

        let ty: RealizedTy<TypeHead> = RealizedTy::List(
            Box::new(RealizedTy::Class(head, vec![], TyAttr::default())),
            TyAttr::default(),
        );

        // The generated walk finds the head through the `Box`.
        let mut seen = Vec::new();
        ty.visit_heads(&mut |h| seen.push(*h));
        assert_eq!(seen, vec![head]);

        // And the widening conversion reinterprets at this head.
        let widened = baml_type::Ty::from(&ty);
        let mut seen_wide = Vec::new();
        widened.visit_heads(&mut |h| seen_wide.push(*h));
        assert_eq!(seen_wide, vec![head]);
    }
}
