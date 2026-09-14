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
/// # Unresolved heads, and serialization
///
/// Because the tag is the whole identity, the pointer is a *cache* of the
/// tag→declaration lookup rather than part of the value. A head is therefore
/// meaningful before that cache is filled: [`unresolved`](Self::unresolved)
/// builds one that compares, orders, and hashes correctly but cannot yet be
/// dereferenced. Emit produces those — there is no heap at compile time — and
/// the loader fills each pointer in via [`resolve`](Self::resolve).
///
/// Serialization follows from the same fact: only the tag is written, and
/// decoding yields an unresolved head. That is not lossy, because the pointer
/// was never identity — a round trip through a file returns *the same head*,
/// just with a cold cache. It is why a compiled `Program` can hold
/// `RealizedTy<TypeHead>` at all despite `Object`'s Borsh impl being
/// context-free: there is nothing to resolve *at decode time*, only at load.
///
/// The cost is that "unresolved" is not distinguishable in the type system, so
/// a head the loader misses fails on its first dereference. Loading therefore
/// ends by asserting no unresolved head survives, and `resolve`/`forward_to`
/// each assert the state they expect, so a double-resolve or a forward of a
/// never-resolved head is caught where it happens rather than downstream.
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

    /// A head that knows *which* declaration it names but not yet *where* it
    /// lives — the form emit produces, since there is no heap at compile time
    /// and the tag alone is the identity. Dereferencing one before
    /// [`resolve`](Self::resolve) is a null dereference.
    #[must_use]
    pub fn unresolved(tag: TypeTag) -> Self {
        debug_assert!(
            tag.is_head(),
            "a type head must carry a declared head's tag, not a primitive",
        );
        Self {
            ptr: HeapPtr::null(),
            tag,
        }
    }

    /// The unresolved head a fully-qualified name denotes.
    ///
    /// Tags for *compiled* declarations are content-addressed from the name, so
    /// this is a pure function — no registry, no heap, nothing to look up. It
    /// is the one place a name becomes a head, so the rendering that feeds the
    /// hash cannot drift between the emitter that mints a declaration's
    /// `type_tag` and the references pointing at it. Both use
    /// `render_dotted(false)`.
    ///
    /// **Compiled declarations only.** A runtime-created declaration's tag
    /// comes from [`TypeTag::fresh_dynamic`], never from its (synthesized,
    /// display-only) name — so a head for one must be built as
    /// `TypeHead::new(just_allocated_ptr, object.type_tag)`, reading the tag
    /// off the declaration. Calling `of_name` with a runtime declaration's
    /// name yields a head that matches nothing.
    #[must_use]
    pub fn of_name(name: &baml_type::TypeName) -> Self {
        Self::unresolved(TypeTag::of_head(&name.render_dotted(false)))
    }

    /// The fully-qualified name of the declaration this head points at, or
    /// `None` if the head is unresolved, does not point at a declaration, or
    /// points at an anonymous (runtime-created) declaration — which has no
    /// qualified name for any name-keyed consumer to use.
    ///
    /// The inverse of [`of_name`](Self::of_name), and the boundary conversion: a
    /// head is a live pointer into this process's heap, so anything leaving the
    /// VM — an FFI payload, a serialized artifact, a host-facing value —
    /// converts back to names first. Pair it with `try_map_heads` to carry a
    /// whole type across.
    #[must_use]
    pub fn declared_name(self) -> Option<baml_type::TypeName> {
        if !self.is_resolved() {
            return None;
        }
        // SAFETY: as in `tagged_name` — the caller holds the heap permit for
        // the read and the collector forwards heads when declarations move
        // (runtime-created declarations live in the moving region; this very
        // function's `Anonymous` arm exists for them). A declaration is
        // immutable after it is created.
        #[expect(
            unsafe_code,
            reason = "recovering a head's name requires reading its declaration"
        )]
        let object = unsafe { self.ptr.get() };
        match object {
            // An anonymous (runtime-created) declaration has no qualified name,
            // so it is unnameable here by design — `to_name` refuses rather
            // than fabricating a spelling nothing declares.
            crate::Object::Class(class) => class.name.declared().cloned(),
            crate::Object::Enum(enm) => enm.name.declared().cloned(),
            crate::Object::Interface(iface) => Some(iface.name.clone()),
            crate::Object::TypeAlias(alias) => Some(alias.name.clone()),
            _ => None,
        }
    }

    /// This head's declaration name, or the head itself when it has none.
    ///
    /// The mapper form of [`declared_name`](Self::declared_name), for pairing
    /// with `try_map_heads` to carry a whole type out of the VM:
    /// `ty.try_map_heads(&mut |head| head.to_name())`.
    ///
    /// # Errors
    ///
    /// [`UnnameableHead`](crate::UnnameableHead) when the head is unresolved or
    /// its declaration has no name a host could look up.
    pub fn to_name(&self) -> Result<baml_type::TypeName, crate::UnnameableHead> {
        self.declared_name().ok_or(crate::UnnameableHead(self.tag))
    }

    /// The per-call seam spelling of this head's declaration: its declared
    /// name, or the bare item name as a local spelling when the declaration is
    /// anonymous. `None` when the head is unresolved or points at no
    /// declaration. See [`DeclarationName::overlay_name`](crate::DeclarationName::overlay_name)
    /// for the contract — this spelling is only meaningful against tables the
    /// same call built.
    #[must_use]
    pub fn overlay_name(self) -> Option<baml_type::TypeName> {
        if !self.is_resolved() {
            return None;
        }
        // SAFETY: as in `declared_name`.
        #[expect(
            unsafe_code,
            reason = "recovering a head's name requires reading its declaration"
        )]
        let object = unsafe { self.ptr.get() };
        match object {
            crate::Object::Class(class) => Some(class.name.overlay_name()),
            crate::Object::Enum(enm) => Some(enm.name.overlay_name()),
            crate::Object::Interface(iface) => Some(iface.name.clone()),
            crate::Object::TypeAlias(alias) => Some(alias.name.clone()),
            _ => None,
        }
    }

    /// This head at the sys-op lane's head: the identity it already carries,
    /// plus the declaration's own name read off the declaration.
    ///
    /// Nothing is fabricated — an anonymous declaration stays anonymous — and
    /// nothing is a pointer, so the result survives the collector moving
    /// objects while a sys-op awaits. `None` when the head is unresolved or
    /// points at something that is not a declaration.
    #[must_use]
    pub fn tagged_name(self) -> Option<baml_type::TaggedTypeName> {
        if !self.is_resolved() {
            return None;
        }
        // SAFETY: as in `declared_name` — the caller holds the heap permit for
        // the read, and a declaration is immutable after it is created.
        #[expect(
            unsafe_code,
            reason = "reading a head's declaration to carry its name off the heap"
        )]
        let object = unsafe { self.ptr.get() };
        let name = match object {
            crate::Object::Class(class) => class.name.clone(),
            crate::Object::Enum(enm) => enm.name.clone(),
            crate::Object::Interface(iface) => {
                baml_type::DeclarationName::Declared(iface.name.clone())
            }
            crate::Object::TypeAlias(alias) => {
                baml_type::DeclarationName::Declared(alias.name.clone())
            }
            _ => return None,
        };
        Some(baml_type::TaggedTypeName::new(self.tag, name))
    }

    /// The mapper form of [`tagged_name`](Self::tagged_name), for pairing with
    /// `try_map_heads` to carry a whole type onto the lane.
    ///
    /// # Errors
    ///
    /// [`UnnameableHead`](crate::UnnameableHead) when the head is unresolved or
    /// does not point at a declaration.
    pub fn to_tagged_name(&self) -> Result<baml_type::TaggedTypeName, crate::UnnameableHead> {
        self.tagged_name().ok_or(crate::UnnameableHead(self.tag))
    }

    /// The mapper form of [`overlay_name`](Self::overlay_name), for pairing
    /// with `try_map_heads` — the per-call twin of [`to_name`](Self::to_name).
    ///
    /// # Errors
    ///
    /// [`UnnameableHead`](crate::UnnameableHead) when the head is unresolved
    /// or does not point at a declaration.
    pub fn to_overlay_name(&self) -> Result<baml_type::TypeName, crate::UnnameableHead> {
        self.overlay_name().ok_or(crate::UnnameableHead(self.tag))
    }

    /// Whether the pointer cache has been filled — false for a head straight out
    /// of emit or a decoder, true once the loader has bound it.
    #[must_use]
    pub fn is_resolved(self) -> bool {
        !self.ptr.as_ptr().is_null()
    }

    /// Fill in the access path for a head that arrived from emit or a decoder.
    ///
    /// The identity does not change — this only populates the cache — so
    /// resolving late, or in any order, is safe. Resolving twice is not: it
    /// means the loader visited one head through two paths and the second write
    /// could disagree with the first.
    pub fn resolve(&mut self, definition: HeapPtr) {
        debug_assert!(
            !self.is_resolved(),
            "a head is resolved once; re-resolving means two loader paths reached it",
        );
        self.ptr = definition;
    }

    /// Where the declaration lives — for dereferencing it, and for the collector
    /// to trace. Not an identity; compare [`tag`](Self::tag) instead.
    ///
    /// Null until [`resolve`](Self::resolve) has run; see
    /// [`is_resolved`](Self::is_resolved).
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
        debug_assert!(
            self.is_resolved(),
            "forwarding an unresolved head: the collector reached a head the loader never bound",
        );
        self.ptr = moved;
    }
}

/// A head names itself by reaching its declaration — the runtime counterpart of
/// a `QualifiedTypeName` answering from the name it already is.
///
/// Unresolved heads, and the tag of something that is not a declaration, render
/// as their tag rather than a guess: a display path must never invent a name,
/// and this is reached from `Display`, which cannot report an error.
impl baml_type::HeadDisplay for TypeHead {
    fn head_display_name(&self) -> String {
        if !self.is_resolved() {
            return format!("<unresolved type #{}>", self.tag.as_i64());
        }
        // SAFETY: a resolved head points at a declaration object. Compiled
        // declarations live in the never-moved compile-time region; a
        // runtime-created one lives in the moving region, where the caller's
        // heap permit keeps the collector parked and head fixup has already
        // forwarded this pointer past any earlier move. Read-only, and
        // declarations are immutable after creation, so no aliasing obligation
        // arises either.
        #[expect(
            unsafe_code,
            reason = "naming a head requires reading the declaration it points at"
        )]
        let object = unsafe { self.ptr.get() };
        match object {
            crate::Object::Class(class) => class.name.display_name().to_string(),
            crate::Object::Enum(enm) => enm.name.display_name().to_string(),
            crate::Object::Interface(iface) => iface.name.display_name().to_string(),
            crate::Object::TypeAlias(alias) => alias.name.display_name().to_string(),
            _ => format!("<type #{}>", self.tag.as_i64()),
        }
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

// Only the tag crosses a boundary — the pointer is a per-process cache of the
// tag→declaration lookup, so writing it would be meaningless and reading it back
// would be unsound. Decoding therefore yields an unresolved head, which the
// loader binds. Hand-written rather than derived so `ptr` cannot be folded in by
// someone adding a `#[derive]`.

impl borsh::BorshSerialize for TypeHead {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.tag.serialize(writer)
    }
}

impl borsh::BorshDeserialize for TypeHead {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let tag = TypeTag::deserialize_reader(reader)?;
        if !tag.is_head() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("type head carries a non-head tag ({})", tag.as_i64()),
            ));
        }
        Ok(Self {
            ptr: HeapPtr::null(),
            tag,
        })
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

    /// Point at `slot`, spanning both shapes of [`HeapPtr::from_ptr`] — the
    /// `heap_debug` build takes an allocation epoch as well. Kept in one place
    /// so the tests below read identically under either feature set.
    fn ptr_to(slot: &mut crate::Object) -> HeapPtr {
        let raw = std::ptr::from_mut(slot);
        // SAFETY: every caller's slot outlives the head built from it, so the
        // pointer stays valid; these tests exercise the relations, never a deref.
        // The epoch is arbitrary for the same reason — nothing reads through it.
        #[cfg(not(feature = "heap_debug"))]
        unsafe {
            HeapPtr::from_ptr(raw)
        }
        #[cfg(feature = "heap_debug")]
        unsafe {
            HeapPtr::from_ptr(raw, 0)
        }
    }

    fn head(slot: &mut crate::Object, tag: i64) -> TypeHead {
        TypeHead::new(ptr_to(slot), TypeTag::from_i64(tag))
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
        after.forward_to(ptr_to(&mut to_space));

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

    /// A head survives serialization as *the same head* with a cold pointer
    /// cache — the property that lets a compiled `Program` carry heap-headed
    /// types even though `Object`'s decoder has no heap to resolve against.
    #[test]
    fn serialization_keeps_identity_and_drops_the_cache() {
        use borsh::BorshDeserialize;

        let mut definition = slot();
        let resolved = TypeHead::new(ptr_to(&mut definition), TypeTag::of_head("demo.Person"));
        assert!(resolved.is_resolved());

        let decoded = TypeHead::try_from_slice(&borsh::to_vec(&resolved).expect("serialize"))
            .expect("decode");
        assert_eq!(decoded, resolved, "a decoded head is the same head");
        assert!(!decoded.is_resolved(), "the pointer cache does not travel");

        // Emit builds heads this way, with no heap in sight: the tag is
        // content-addressed from the name, so it needs nothing to look up.
        let name = baml_type::TypeName::new(
            baml_base::Name::new("demo"),
            vec![],
            baml_base::Name::new("Person"),
        );
        assert_eq!(TypeHead::of_name(&name), resolved);
        assert!(!TypeHead::of_name(&name).is_resolved());

        // And the loader binds it without disturbing identity.
        let mut loaded = TypeHead::of_name(&name);
        loaded.resolve(ptr_to(&mut definition));
        assert_eq!(loaded, resolved);
        assert!(loaded.is_resolved());

        // A non-head tag on the wire is rejected rather than producing a head
        // that names a primitive.
        let primitive = borsh::to_vec(&TypeTag::from_i64(baml_type::typetag::INT)).expect("encode");
        assert!(TypeHead::try_from_slice(&primitive).is_err());
    }

    /// A heap-headed type renders by reaching its declaration, so the runtime
    /// gets real names out of `Display` without a package-map lookup — and an
    /// unresolved head degrades to its tag instead of inventing one.
    #[test]
    fn a_heap_headed_type_renders_through_its_declaration() {
        use baml_type::{RealizedTy, TyAttr};

        let name = baml_type::TypeName::new(
            baml_base::Name::new("demo"),
            vec![],
            baml_base::Name::new("Person"),
        );
        let mut declaration = crate::Object::Class(Box::new(crate::types::Class {
            name: crate::DeclarationName::Declared(name.clone()),
            fields: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::default(),
            type_tag: TypeTag::of_head(&name.render_dotted(false)),
            ty_attr: TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: crate::HeapPtr::null(),
        }));

        let mut head = TypeHead::of_name(&name);
        let unresolved: RealizedTy<TypeHead> =
            RealizedTy::Class(head, Box::new([]), TyAttr::default());
        assert_eq!(
            unresolved.to_string(),
            format!("<unresolved type #{}>", head.tag().as_i64()),
        );

        head.resolve(ptr_to(&mut declaration));
        let ty: RealizedTy<TypeHead> = RealizedTy::List(
            Box::new(RealizedTy::Class(head, Box::new([]), TyAttr::default())),
            TyAttr::default(),
        );
        assert_eq!(ty.to_string(), "demo.Person[]");
    }

    /// A runtime-created declaration is *anonymous*: it carries an item name
    /// and nothing else — no package, no namespace path. So the strict
    /// outbound conversion refuses to name it (inventing a spelling is how a
    /// runtime declaration would come to impersonate a compiled one), the
    /// per-call seam spells it locally so a call's own tables can key on it,
    /// and rendering shows what the user called it.
    #[test]
    fn an_anonymous_declaration_has_no_qualified_name() {
        use baml_type::{RealizedTy, TyAttr};

        let type_tag = TypeTag::fresh_dynamic();
        let mut declaration = crate::Object::Class(Box::new(crate::types::Class {
            name: crate::DeclarationName::Anonymous(baml_base::Name::new("Widget")),
            fields: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::default(),
            type_tag,
            ty_attr: TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: crate::HeapPtr::null(),
        }));
        let head = TypeHead::new(ptr_to(&mut declaration), type_tag);

        assert_eq!(head.declared_name(), None);
        assert!(head.to_name().is_err());

        let overlay = head
            .overlay_name()
            .expect("an anonymous head still spells locally");
        assert!(overlay.is_local());
        assert!(overlay.namespace().is_empty());
        assert_eq!(overlay.name().as_str(), "Widget");

        let ty: RealizedTy<TypeHead> = RealizedTy::Class(head, Box::new([]), TyAttr::default());
        assert_eq!(ty.to_string(), "Widget");
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
        let head = TypeHead::new(ptr_to(&mut definition), TypeTag::of_head("demo.Person"));

        let ty: RealizedTy<TypeHead> = RealizedTy::List(
            Box::new(RealizedTy::Class(head, Box::new([]), TyAttr::default())),
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

#[cfg(test)]
mod head_trait {
    /// `TypeHead` satisfies the algebra's `Head` contract, so `RealizedTy<TypeHead>`
    /// can be normalized/compared by the same code the compiler uses. Nothing
    /// implements it explicitly — `Head` is blanket-implemented over its bounds,
    /// so this asserts the bounds are actually met rather than assumed.
    #[test]
    fn type_head_is_a_head() {
        fn assert_head<H: baml_type::Head>() {}
        assert_head::<super::TypeHead>();
    }
}
