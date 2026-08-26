//! BEP-066 mounted-package linking: the enriched `PackageInterface` export schema.
//!
//! These tests pin the *derivation* of the new loc-free rows — interface
//! surfaces (params/bounds/`requires`/associated types/fields/methods), class
//! field attributes and per-parameter bounds, function bounds and stable
//! fully-qualified names, and the full namespace set — plus borsh round-trip
//! and cross-database byte determinism. The mounted-package integration suites
//! additionally pin their source-less type, call, runtime, and artifact
//! consumption; these focused tests keep the derivation contract readable.

use baml_base::Name;
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::{
    package_interface::{
        ExportedAssociatedType, ExportedFunction, ExportedImpl, ExportedImplMethod,
        ExportedImplOrigin, ExportedType, PackageInterface, package_interface,
    },
    ty::Ty,
};
use baml_db::{ProjectDatabase, testing::assert_no_diagnostic_errors};

use super::support::make_db;
use crate::engine::TestDbExt;

const FIXTURE: &str = r#"
interface Anchor {
    function id(self) -> string throws never
}

interface Source {
    type Item extends Anchor
    type Items = Self.Item[]

    label string @alias("lbl") @description("source label")

    function next(self) -> Self.Item throws never

    function twice(self) -> Self.Item[] throws never {
        [self.next(), self.next()]
    }
}

interface Feed requires Source<Item = Self.Item> {
    type Item extends Anchor
}

interface Pair<A, B> {
    function first(self) -> A throws never
}

interface Swapped<A, B> requires Pair<B, A> {
}

class Card {
    title string @alias("t") @description("card title")

    implements Anchor {
        function id(self) -> string throws never {
            self.title
        }
    }
}

class Box<T extends Anchor> {
    item T
}

function pick<T extends Anchor>(a: T, b: T) -> T throws never {
    a
}

interface Mergeable {
    function merge(self, other: Self) -> Self throws never
}

enum Tone {
    Soft
    Loud
}

implement Mergeable for Tone {
    function merge(self, other: Self) -> Self throws never {
        self
    }
}

interface Labeled {
    tag string
}

class Sticker {
    kind string

    implements Labeled {
        tag as kind
    }
}

implement<T extends Anchor> Pair<T, T> for Box<T> {
    function first(self) -> T throws never {
        self.item
    }
}

class Deck {
    label string
    top Card

    implements Source {
        type Item = Card

        function next(self) -> Card throws never {
            self.top
        }
    }
}

interface Tagged {
    function tag(self) -> string throws never
}

implement<T> Tagged for T {
    function tag(self) -> string throws never {
        "any"
    }
}
"#;

fn fixture_db() -> ProjectDatabase {
    let mut db = make_db();
    db.file("main.baml", FIXTURE);
    db.file(
        "ns_util/helpers.baml",
        "function helper() -> int throws never {\n    1\n}\n",
    );
    // A namespace whose ONLY item is an interface: before interface export its
    // namespace was invisible in the interface (interfaces were dropped and
    // nothing else exported from it).
    db.file("ns_shapes/only_iface.baml", "interface Marker {\n}\n");
    db
}

fn user_interface(db: &ProjectDatabase) -> &PackageInterface {
    package_interface(db, PackageId::new(db, Name::new("user")))
}

#[track_caller]
fn lookup<'a>(iface: &'a PackageInterface, ns: &[&str], name: &str) -> &'a ExportedType {
    let ns: Vec<Name> = ns.iter().map(|part| Name::new(*part)).collect();
    iface
        .lookup_type(&ns, &Name::new(name))
        .unwrap_or_else(|| panic!("{name} is exported"))
}

#[track_caller]
fn assoc<'a>(rows: &'a [ExportedAssociatedType], name: &str) -> &'a ExportedAssociatedType {
    rows.iter()
        .find(|row| row.name.as_str() == name)
        .unwrap_or_else(|| panic!("associated type {name} is exported"))
}

#[track_caller]
fn method<'a>(methods: &'a [ExportedFunction], name: &str) -> &'a ExportedFunction {
    methods
        .iter()
        .find(|m| m.name.as_str() == name)
        .unwrap_or_else(|| panic!("method {name} is exported"))
}

/// The unique exported impl of `interface_name` whose for-type satisfies
/// `for_ty` — panics on zero or several (the fixtures are written so each
/// lookup is unambiguous).
#[track_caller]
fn impl_row<'a>(
    iface: &'a PackageInterface,
    interface_name: &str,
    for_ty: impl Fn(&Ty) -> bool,
) -> &'a ExportedImpl {
    let mut rows = iface.impls.iter().filter(|row| {
        row.interface.name.name().as_str() == interface_name && for_ty(&row.for_ty_pattern)
    });
    let found = rows
        .next()
        .unwrap_or_else(|| panic!("an impl of {interface_name} is exported"));
    assert!(
        rows.next().is_none(),
        "impl of {interface_name} matched more than one exported row"
    );
    found
}

#[track_caller]
fn impl_method<'a>(methods: &'a [ExportedImplMethod], name: &str) -> &'a ExportedImplMethod {
    methods
        .iter()
        .find(|m| m.name.as_str() == name)
        .unwrap_or_else(|| panic!("impl method {name} is exported"))
}

fn is_class_named(name: &'static str) -> impl Fn(&Ty) -> bool {
    move |ty| matches!(ty, Ty::Class(qtn, ..) if qtn.name().as_str() == name)
}

#[test]
fn interface_export_carries_full_symbolic_surface() {
    let db = fixture_db();
    assert_no_diagnostic_errors(&db);
    let iface = user_interface(&db);

    let ExportedType::Interface {
        qtn,
        self_param,
        generic_params,
        param_bounds,
        requires,
        associated_types,
        fields,
        required_methods,
        default_methods,
    } = lookup(iface, &[], "Source")
    else {
        panic!("Source must export as ExportedType::Interface");
    };

    assert_eq!(qtn.name().as_str(), "Source");
    assert_eq!(qtn.package().as_str(), "user");
    assert_eq!(self_param.as_str(), "Self");
    assert!(generic_params.is_empty());
    assert!(param_bounds.is_empty());
    assert!(requires.is_empty(), "Source requires nothing");

    // `type Item extends Anchor` — bound realized, no default.
    let item = assoc(associated_types, "Item");
    let bound = item.bound.as_ref().expect("Item carries its Anchor bound");
    assert_eq!(bound.name.name().as_str(), "Anchor");
    assert!(item.default.is_none());

    // `type Items = Self.Item[]` — default pre-lowered with symbolic `Self`:
    // a list of a projection onto the symbolic receiver.
    let items = assoc(associated_types, "Items");
    assert!(items.bound.is_none());
    let default = items.default.as_ref().expect("Items carries its default");
    let Ty::List(inner, _) = default else {
        panic!("Items default must lower to a list, got {default:?}");
    };
    let Ty::AssociatedTypeProjection {
        base,
        interface,
        member,
        ..
    } = inner.as_ref()
    else {
        panic!("Items default element must stay a symbolic projection, got {inner:?}");
    };
    assert_eq!(member.as_str(), "Item");
    assert_eq!(interface.name.name().as_str(), "Source");
    assert!(
        matches!(base.as_ref(), Ty::TypeVar(p, _) if p.as_str() == "Self"),
        "projection base must be the symbolic Self type variable"
    );

    // Fields with schema attributes, resolved in the interface's own scope.
    let (field_name, field_ty, attrs) = &fields[0];
    assert_eq!(field_name.as_str(), "label");
    assert!(matches!(field_ty, Ty::String { .. }));
    assert_eq!(attrs.alias.as_deref(), Some("lbl"));
    assert_eq!(attrs.description.as_deref(), Some("source label"));

    // Required AND default methods (exported split), signatures with
    // symbolic `Self`.
    let next = method(required_methods, "next");
    assert!(
        matches!(&next.params[0].ty, Ty::TypeVar(p, _) if p.as_str() == "Self"),
        "required-method receiver stays the symbolic Self"
    );
    assert!(
        matches!(&next.return_type, Ty::AssociatedTypeProjection { member, .. }
            if member.as_str() == "Item")
    );
    assert!(matches!(&next.declared_throws, Some(Ty::Never { .. })));
    assert!(matches!(&next.callable_throws, Ty::Never { .. }));
    assert_eq!(next.callable_fqn, "user.Source.next");
    assert!(next.interface_target.is_none());

    let twice = method(default_methods, "twice");
    assert!(
        matches!(&twice.params[0].ty, Ty::TypeVar(p, _) if p.as_str() == "Self"),
        "default-method receiver stays the symbolic Self"
    );
    assert!(matches!(&twice.return_type, Ty::List(inner, _)
            if matches!(inner.as_ref(), Ty::AssociatedTypeProjection { member, .. }
                if member.as_str() == "Item")));
    assert!(matches!(&twice.callable_throws, Ty::Never { .. }));
    assert_eq!(twice.callable_fqn, "user.Source.twice");
}

#[test]
fn interface_requires_closure_is_pre_realized() {
    let db = fixture_db();
    let iface = user_interface(&db);

    // `Feed requires Source<Item = Self.Item>` — the pinning idiom: the pin
    // must survive as a symbolic projection onto Feed's own `Self`.
    let ExportedType::Interface { requires, .. } = lookup(iface, &[], "Feed") else {
        panic!("Feed must export as ExportedType::Interface");
    };
    assert_eq!(requires.len(), 1, "Feed's closure is exactly Source");
    let source = &requires[0];
    assert_eq!(source.name.name().as_str(), "Source");
    assert!(source.generics.is_empty());
    let (pin_name, pin_ty) = source
        .associated_types
        .iter()
        .find(|(name, _)| name.as_str() == "Item")
        .expect("the Item pin is carried");
    assert_eq!(pin_name.as_str(), "Item");
    let Ty::AssociatedTypeProjection {
        interface, member, ..
    } = pin_ty
    else {
        panic!("Item pin must stay a symbolic Self.Item projection, got {pin_ty:?}");
    };
    assert_eq!(member.as_str(), "Item");
    assert_eq!(
        interface.name.name().as_str(),
        "Feed",
        "the projection is onto the REQUIRING interface's symbolic Self"
    );

    // `Swapped<A, B> requires Pair<B, A>` — generic-argument substitution at
    // identity args: the parent entry carries the child's params, swapped.
    let ExportedType::Interface {
        generic_params,
        param_bounds,
        requires,
        ..
    } = lookup(iface, &[], "Swapped")
    else {
        panic!("Swapped must export as ExportedType::Interface");
    };
    assert_eq!(
        generic_params
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["A", "B"]
    );
    assert_eq!(param_bounds.len(), 2);
    assert!(param_bounds.iter().all(Vec::is_empty));
    assert_eq!(requires.len(), 1);
    let pair = &requires[0];
    assert_eq!(pair.name.name().as_str(), "Pair");
    let arg_names: Vec<&str> = pair
        .generics
        .iter()
        .map(|arg| match arg {
            Ty::TypeVar(p, _) => p.as_str(),
            other => panic!("identity-arg substitution must yield type vars, got {other:?}"),
        })
        .collect();
    assert_eq!(arg_names, ["B", "A"]);
}

#[test]
fn class_export_carries_attrs_bounds_and_interface_targets() {
    let db = fixture_db();
    let iface = user_interface(&db);

    let ExportedType::Class {
        fields, methods, ..
    } = lookup(iface, &[], "Card")
    else {
        panic!("Card must export as ExportedType::Class");
    };
    let (field_name, _, attrs) = &fields[0];
    assert_eq!(field_name.as_str(), "title");
    assert_eq!(attrs.alias.as_deref(), Some("t"));
    assert_eq!(attrs.description.as_deref(), Some("card title"));

    let id = method(methods, "id");
    let target = id
        .interface_target
        .as_ref()
        .expect("implements-block method carries its interface target");
    assert_eq!(target.name().as_str(), "Anchor");
    assert_eq!(target.package().as_str(), "user");
    assert_eq!(id.callable_fqn, "user.Card.id");

    let ExportedType::Class {
        generic_params,
        generic_param_bounds,
        ..
    } = lookup(iface, &[], "Box")
    else {
        panic!("Box must export as ExportedType::Class");
    };
    assert_eq!(generic_params.len(), 1);
    assert_eq!(generic_params[0].as_str(), "T");
    assert_eq!(generic_param_bounds.len(), 1);
    assert_eq!(generic_param_bounds[0].len(), 1);
    assert_eq!(generic_param_bounds[0][0].name.name().as_str(), "Anchor");
}

#[test]
fn function_export_carries_bounds_and_fqn() {
    let db = fixture_db();
    let iface = user_interface(&db);

    let pick = iface
        .lookup_function(&[], &Name::new("pick"))
        .expect("pick is exported");
    assert_eq!(pick.generic_params.len(), 1);
    assert_eq!(pick.generic_params[0].as_str(), "T");
    assert_eq!(
        pick.generic_param_bounds.len(),
        pick.generic_params.len(),
        "bounds are parallel to generic_params"
    );
    assert_eq!(pick.generic_param_bounds[0].len(), 1);
    assert_eq!(
        pick.generic_param_bounds[0][0].name.name().as_str(),
        "Anchor"
    );
    assert_eq!(pick.callable_fqn, "user.pick");
    assert!(pick.interface_target.is_none());

    let helper = iface
        .lookup_function(&[Name::new("util")], &Name::new("helper"))
        .expect("namespaced helper is exported");
    assert_eq!(helper.callable_fqn, "user.util.helper");
    assert!(helper.generic_param_bounds.is_empty());
}

#[test]
fn namespace_set_is_complete() {
    let db = fixture_db();
    let iface = user_interface(&db);

    let ns = |parts: &[&str]| -> Vec<Name> { parts.iter().map(|p| Name::new(*p)).collect() };
    assert!(
        iface.namespaces.contains(&ns(&[])),
        "root namespace present"
    );
    assert!(iface.namespaces.contains(&ns(&["util"])));
    assert!(
        iface.namespaces.contains(&ns(&["shapes"])),
        "a namespace whose only item is an interface is still in the set"
    );

    // And the interface-only namespace now has a structural row too.
    assert!(matches!(
        lookup(iface, &["shapes"], "Marker"),
        ExportedType::Interface { .. }
    ));
}

#[test]
fn enriched_interface_borsh_round_trips() {
    let db = fixture_db();
    let iface = user_interface(&db);

    assert!(
        !iface.impls.is_empty(),
        "the fixture's impls participate in the round-trip"
    );
    let bytes = borsh::to_vec(iface).expect("serialize enriched interface");
    let decoded: PackageInterface = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(iface, &decoded);

    // The stdlib exercises the gnarly idioms — round-trip it too.
    let stdlib = package_interface(&db, PackageId::new(&db, Name::new("baml")));
    let bytes = borsh::to_vec(stdlib).expect("serialize stdlib interface");
    let decoded: PackageInterface = borsh::from_slice(&bytes).expect("deserialize stdlib");
    assert_eq!(stdlib, &decoded);
}

#[test]
fn enriched_interface_derivation_is_deterministic() {
    // Two independently built databases over the same sources must produce
    // byte-identical borsh — the soundness foundation for B-694-style caching
    // of the enriched schema.
    let db1 = fixture_db();
    let db2 = fixture_db();

    let user1 = borsh::to_vec(user_interface(&db1)).expect("serialize");
    let user2 = borsh::to_vec(user_interface(&db2)).expect("serialize");
    assert_eq!(
        user1, user2,
        "user package derivation must be deterministic"
    );

    let baml1 = borsh::to_vec(package_interface(
        &db1,
        PackageId::new(&db1, Name::new("baml")),
    ))
    .expect("serialize");
    let baml2 = borsh::to_vec(package_interface(
        &db2,
        PackageId::new(&db2, Name::new("baml")),
    ))
    .expect("serialize");
    assert_eq!(baml1, baml2, "stdlib derivation must be deterministic");
}

#[test]
fn stdlib_interfaces_derive_enriched() {
    let db = make_db();

    // Every stdlib package derives its enriched interface without panicking.
    for name in baml_builtins2::stdlib_package_names().iter().copied() {
        let iface = package_interface(&db, PackageId::new(&db, Name::new(name)));
        assert!(
            iface.namespaces.contains(&Vec::new()),
            "{name} has a root namespace"
        );
    }

    // Spot-check the gnarliest idiom: `Iterator requires
    // Iterable<Item = Self.Item, Error = Self.Error>`.
    let baml = package_interface(&db, PackageId::new(&db, Name::new("baml")));
    let ExportedType::Interface {
        requires,
        associated_types,
        required_methods,
        default_methods,
        ..
    } = lookup(baml, &["iter"], "Iterator")
    else {
        panic!("baml.iter.Iterator must export as ExportedType::Interface");
    };

    let iterable = requires
        .iter()
        .find(|req| req.name.name().as_str() == "Iterable")
        .expect("Iterator's closure contains Iterable");
    assert_eq!(iterable.name.namespace(), &[Name::new("iter")]);
    for pin in ["Item", "Error"] {
        let (_, pin_ty) = iterable
            .associated_types
            .iter()
            .find(|(name, _)| name.as_str() == pin)
            .unwrap_or_else(|| panic!("Iterable pin {pin} is carried"));
        assert!(
            matches!(pin_ty, Ty::AssociatedTypeProjection { interface, member, .. }
                if member.as_str() == pin && interface.name.name().as_str() == "Iterator"),
            "{pin} pin must stay a symbolic Self.{pin} projection onto Iterator, got {pin_ty:?}"
        );
    }

    let item = assoc(associated_types, "Item");
    assert!(item.default.is_none(), "Item declares no default");
    let error = assoc(associated_types, "Error");
    assert!(
        matches!(error.default.as_ref(), Some(Ty::Never { .. })),
        "`type Error = never` exports its default"
    );

    let next = method(required_methods, "next");
    assert_eq!(next.callable_fqn, "baml.iter.Iterator.next");
    assert!(matches!(&next.params[0].ty, Ty::TypeVar(p, _) if p.as_str() == "Self"));
    let map = method(default_methods, "map");
    assert_eq!(
        map.generic_params
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["R", "E2"],
        "default-method generics survive with symbolic-Self signatures"
    );
    assert_eq!(map.generic_param_bounds.len(), map.generic_params.len());
}

#[test]
fn self_in_requirement_generic_args_and_bounds_stays_symbolic() {
    // `Self` written inside a `requires` clause's generic ARGUMENTS (not an
    // associated-type binding) must export as the symbolic rigid `Self` — the
    // closure walker previously lowered these argument positions with no
    // `Self` in scope, silently producing `Ty::Error`. The bound positions
    // (interface-level `T extends Parent<Self>` and method-level
    // `f<U extends Parent<Self>>`) already resolved symbolically — pinned
    // here so they cannot regress.
    let mut db = make_db();
    db.file(
        "self_shapes.baml",
        r#"
interface Parent<P> {
}

interface Crtp requires Parent<Self> {
}

interface Projected requires Parent<Self.Item> {
    type Item
}

interface Recur<T extends Parent<Self>> {
}

interface M {
    function f<U extends Parent<Self>>(self, u: U) -> int throws never
}
"#,
    );
    assert_no_diagnostic_errors(&db);
    let iface = user_interface(&db);

    // `requires Parent<Self>` — the CRTP shape: the argument is the symbolic
    // rigid `Self` type variable.
    let ExportedType::Interface { requires, .. } = lookup(iface, &[], "Crtp") else {
        panic!("Crtp must export as ExportedType::Interface");
    };
    assert_eq!(requires.len(), 1);
    assert_eq!(requires[0].name.name().as_str(), "Parent");
    assert!(
        matches!(&requires[0].generics[0], Ty::TypeVar(p, _) if p.as_str() == "Self"),
        "requirement generic arg `Self` must stay the symbolic Self, got {:?}",
        requires[0].generics[0]
    );

    // `requires Parent<Self.Item>` — the argument is a symbolic projection
    // onto the REQUIRING interface's `Self` (unpinned at the declaration, so
    // it must not collapse).
    let ExportedType::Interface { requires, .. } = lookup(iface, &[], "Projected") else {
        panic!("Projected must export as ExportedType::Interface");
    };
    assert_eq!(requires.len(), 1);
    assert_eq!(requires[0].name.name().as_str(), "Parent");
    let arg = &requires[0].generics[0];
    assert!(
        matches!(arg, Ty::AssociatedTypeProjection { base, interface, member, .. }
            if member.as_str() == "Item"
                && interface.name.name().as_str() == "Projected"
                && matches!(base.as_ref(), Ty::TypeVar(p, _) if p.as_str() == "Self")),
        "requirement generic arg `Self.Item` must stay a symbolic projection, got {arg:?}"
    );

    // Interface-level param bound using `Self` (already symbolic — pinned).
    let ExportedType::Interface { param_bounds, .. } = lookup(iface, &[], "Recur") else {
        panic!("Recur must export as ExportedType::Interface");
    };
    assert!(
        matches!(&param_bounds[0][0].generics[0], Ty::TypeVar(p, _) if p.as_str() == "Self"),
        "param-bound `Self` must stay symbolic, got {:?}",
        param_bounds[0][0].generics[0]
    );

    // Method-level generic bound using `Self` (already symbolic — pinned).
    let ExportedType::Interface {
        required_methods, ..
    } = lookup(iface, &[], "M")
    else {
        panic!("M must export as ExportedType::Interface");
    };
    let f = method(required_methods, "f");
    assert!(
        matches!(&f.generic_param_bounds[0][0].generics[0], Ty::TypeVar(p, _) if p.as_str() == "Self"),
        "method-bound `Self` must stay symbolic, got {:?}",
        f.generic_param_bounds[0][0].generics[0]
    );
}

// ── Exported implementation rows ───────────────────────────────────────────

#[test]
fn in_body_impl_exports_with_field_links() {
    let db = fixture_db();
    assert_no_diagnostic_errors(&db);
    let iface = user_interface(&db);

    // `class Sticker { implements Labeled { tag as kind } }` — the in-body
    // shape with an explicit field link and no overrides.
    let row = impl_row(iface, "Labeled", is_class_named("Sticker"));
    assert_eq!(row.interface.name.name().as_str(), "Labeled");
    assert_eq!(row.interface.name.package().as_str(), "user");
    assert!(row.interface.generics.is_empty());
    assert!(row.interface.associated_types.is_empty());
    assert!(
        matches!(&row.for_ty_pattern, Ty::Class(qtn, args, _)
            if qtn.name().as_str() == "Sticker" && args.is_empty()),
        "in-body impl of a non-generic class normalizes to the bare class, got {:?}",
        row.for_ty_pattern
    );
    assert!(row.generic_params.is_empty());
    assert!(row.param_bounds.is_empty());
    assert!(row.associated_types.is_empty());
    assert_eq!(
        row.field_links
            .iter()
            .map(|(i, c)| (i.as_str(), c.as_str()))
            .collect::<Vec<_>>(),
        [("tag", "kind")],
        "the explicit `tag as kind` link is exported"
    );
    let ExportedImplOrigin::InBodyClass { class_qtn } = &row.origin else {
        panic!(
            "in-body impl must export InBodyClass origin, got {:?}",
            row.origin
        );
    };
    assert_eq!(class_qtn.name().as_str(), "Sticker");
    assert!(row.methods.is_empty(), "no overrides were written");

    // `class Card { implements Anchor { … } }` — in-body with an override:
    // the method row carries the class-qualified fqn and the interface target.
    let card = impl_row(iface, "Anchor", is_class_named("Card"));
    assert!(
        matches!(&card.origin, ExportedImplOrigin::InBodyClass { class_qtn }
        if class_qtn.name().as_str() == "Card")
    );
    let id = impl_method(&card.methods, "id");
    assert_eq!(id.sig.name.as_str(), "id");
    assert_eq!(id.sig.callable_fqn, "user.Card.id");
    assert_eq!(
        id.sig
            .interface_target
            .as_ref()
            .expect("impl methods always carry their interface target")
            .name()
            .as_str(),
        "Anchor"
    );
    assert!(
        matches!(&id.sig.params[0].ty, Ty::Class(qtn, ..) if qtn.name().as_str() == "Card"),
        "the receiver realizes to the for-type, got {:?}",
        id.sig.params[0].ty
    );
    assert!(matches!(&id.sig.return_type, Ty::String { .. }));
}

#[test]
fn out_of_body_impl_realizes_self_to_the_for_type() {
    let db = fixture_db();
    let iface = user_interface(&db);

    // `implement Mergeable for Tone` — a free impl on an enum; `Self` in the
    // override's signature (receiver, parameter, AND return position) must
    // realize to the for-type, never leak as `Ty::Error`/`Ty::Unknown` (the
    // `function_signature_ty` free-impl deficiency this export bypasses).
    let row = impl_row(
        iface,
        "Mergeable",
        |ty| matches!(ty, Ty::Enum(qtn, _) if qtn.name().as_str() == "Tone"),
    );
    assert!(matches!(row.origin, ExportedImplOrigin::OutOfBody));
    assert!(row.generic_params.is_empty());
    assert!(row.associated_types.is_empty());
    assert!(row.field_links.is_empty());

    let merge = impl_method(&row.methods, "merge");
    let is_tone = |ty: &Ty| matches!(ty, Ty::Enum(qtn, _) if qtn.name().as_str() == "Tone");
    assert_eq!(merge.sig.params.len(), 2);
    assert!(
        is_tone(&merge.sig.params[0].ty),
        "self: {:?}",
        merge.sig.params[0].ty
    );
    assert!(
        is_tone(&merge.sig.params[1].ty),
        "other: Self: {:?}",
        merge.sig.params[1].ty
    );
    assert!(
        is_tone(&merge.sig.return_type),
        "-> Self: {:?}",
        merge.sig.return_type
    );
    assert!(matches!(&merge.sig.declared_throws, Some(Ty::Never { .. })));
    assert!(matches!(&merge.sig.callable_throws, Ty::Never { .. }));
    assert!(merge.sig.generic_params.is_empty());
    // A free-impl method's fqn renders owner-less — identity is the structural
    // (impl, name) pair, and MIR's scoped symbol is reconstructed downstream.
    assert_eq!(merge.sig.callable_fqn, "user.merge");
}

#[test]
fn generic_impl_exports_params_bounds_and_patterns() {
    let db = fixture_db();
    let iface = user_interface(&db);

    // `implement<T extends Anchor> Pair<T, T> for Box<T>`.
    let row = impl_row(iface, "Pair", is_class_named("Box"));
    assert!(matches!(row.origin, ExportedImplOrigin::OutOfBody));
    assert_eq!(
        row.generic_params
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["T"]
    );
    assert_eq!(
        row.param_bounds.len(),
        1,
        "bounds are parallel to generic_params"
    );
    assert_eq!(row.param_bounds[0].len(), 1);
    assert_eq!(row.param_bounds[0][0].name.name().as_str(), "Anchor");

    // The interface head carries the impl's args over its own params.
    assert_eq!(row.interface.generics.len(), 2);
    for arg in &row.interface.generics {
        assert!(
            matches!(arg, Ty::TypeVar(p, _) if p.as_str() == "T"),
            "interface arg stays the impl's param, got {arg:?}"
        );
    }
    let Ty::Class(qtn, args, _) = &row.for_ty_pattern else {
        panic!(
            "for-ty must be the Box pattern, got {:?}",
            row.for_ty_pattern
        );
    };
    assert_eq!(qtn.name().as_str(), "Box");
    assert_eq!(args.len(), 1);
    assert!(matches!(&args[0], Ty::TypeVar(p, _) if p.as_str() == "T"));

    // The override's signature stays parametric over the impl's `T`.
    let first = impl_method(&row.methods, "first");
    assert!(
        matches!(&first.sig.params[0].ty, Ty::Class(qtn, args, _)
            if qtn.name().as_str() == "Box"
                && matches!(&args[0], Ty::TypeVar(p, _) if p.as_str() == "T")),
        "receiver realizes to Box<T>, got {:?}",
        first.sig.params[0].ty
    );
    assert!(matches!(&first.sig.return_type, Ty::TypeVar(p, _) if p.as_str() == "T"));
    assert!(
        first.sig.generic_params.is_empty(),
        "the impl's params live on the row, not the method"
    );
}

#[test]
fn dep_interface_rows_resolve_only_for_mounted_packages() {
    // BEP-066 mounted-package linking: an `ExportedType::Interface` row resolves through both
    // `PackageResolutionContext::resolve_type` paths ONLY when the dependency
    // is a MOUNTED (source-less) package — its rows are the sole
    // representation. A source-backed dependency's interface rows stay
    // invisible (its interfaces resolve through source items on the loc path),
    // preserving pre-export resolution for every source-backed dep.
    use baml_compiler2_tir::package_interface::package_resolution_context;

    let mut db = make_db();
    db.file(
        "main.baml",
        "function f() -> int throws never {\n    1\n}\n",
    );
    db.set_mounted_packages(
        [(
            "app".to_string(),
            mounted::app_blob(&[(
                "lib.baml",
                "interface Marker {\n    function id(self) -> string throws never\n}\n",
            )]),
        )]
        .into(),
    )
    .unwrap();
    let res_ctx = package_resolution_context(&db, PackageId::new(&db, Name::new("user")));

    let path = |parts: &[&str]| -> Vec<Name> { parts.iter().map(|p| Name::new(*p)).collect() };

    // The source-backed row IS exported…
    let baml = package_interface(&db, PackageId::new(&db, Name::new("baml")));
    assert!(
        matches!(
            baml.lookup_type(&[Name::new("iter")], &Name::new("Iterator")),
            Some(ExportedType::Interface { .. })
        ),
        "baml.iter.Iterator is exported as an interface row"
    );

    // …but the package-prefixed dep search must not resolve through it
    // (baml has source; its interfaces resolve via source items).
    assert!(
        res_ctx
            .resolve_type(&db, &path(&["baml", "iter", "Iterator"]), &[])
            .is_none(),
        "source-backed dep interface row must stay invisible to package-prefixed resolution"
    );
    assert!(
        res_ctx
            .resolve_type(&db, &path(&["iter", "Iterator"]), &[])
            .is_none(),
        "source-backed dep interface row must stay invisible to the own-then-deps walk"
    );

    // A MOUNTED dependency's interface row resolves on both paths.
    let (_, ty) = res_ctx
        .resolve_type(&db, &path(&["app", "Marker"]), &[])
        .expect("mounted interface row resolves via the package-prefixed path");
    assert!(matches!(ty, Ty::Interface(ref qtn, _, _, _) if qtn.name().as_str() == "Marker"));

    // Contrast: a dep CLASS in the same namespace still resolves through the
    // interface rows on both paths — the gate is interface-specific.
    let (_, ty) = res_ctx
        .resolve_type(&db, &path(&["baml", "iter", "Range"]), &[])
        .expect("dep class resolves via the package-prefixed path");
    assert!(matches!(ty, Ty::Class(ref qtn, _, _) if qtn.name().as_str() == "Range"));
    let (_, ty) = res_ctx
        .resolve_type(&db, &path(&["iter", "Range"]), &[])
        .expect("dep class resolves via the own-then-deps walk");
    assert!(matches!(ty, Ty::Class(ref qtn, _, _) if qtn.name().as_str() == "Range"));
}

#[test]
fn impl_assoc_bindings_export_pins_and_filled_defaults() {
    let db = fixture_db();
    let iface = user_interface(&db);

    // `class Deck { implements Source { type Item = Card … } }` — the explicit
    // pin plus the interface's `type Items = Self.Item[]` default, filled at
    // this impl's receiver.
    let row = impl_row(iface, "Source", is_class_named("Deck"));
    assert!(
        matches!(&row.origin, ExportedImplOrigin::InBodyClass { class_qtn }
        if class_qtn.name().as_str() == "Deck")
    );

    // Rows in interface declaration order: Item (pinned), Items (defaulted).
    assert_eq!(
        row.associated_types
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Item", "Items"]
    );
    let (_, item) = &row.associated_types[0];
    assert!(matches!(item, Ty::Class(qtn, ..) if qtn.name().as_str() == "Card"));
    // The default keeps its `Self.Item` projection symbolic over the receiver
    // (the runtime/consumer reduces it back through this same impl).
    let (_, items) = &row.associated_types[1];
    let Ty::List(inner, _) = items else {
        panic!("Items must fill as a list, got {items:?}");
    };
    assert!(
        matches!(inner.as_ref(), Ty::AssociatedTypeProjection { base, member, .. }
            if member.as_str() == "Item"
                && matches!(base.as_ref(), Ty::Class(qtn, ..) if qtn.name().as_str() == "Deck")),
        "Items default stays a projection on the receiver, got {inner:?}"
    );

    // The interface head carries the same bindings, sorted by name.
    assert_eq!(row.interface.associated_types.len(), 2);
    assert_eq!(
        row.interface
            .associated_types
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Item", "Items"]
    );

    // The override types against the pinned assoc.
    let next = impl_method(&row.methods, "next");
    assert!(matches!(&next.sig.return_type, Ty::Class(qtn, ..)
        if qtn.name().as_str() == "Card"));
    assert_eq!(next.sig.callable_fqn, "user.Deck.next");
}

#[test]
fn blanket_impl_exports_the_bare_typevar_pattern() {
    let db = fixture_db();
    let iface = user_interface(&db);

    // `implement<T> Tagged for T`.
    let row = impl_row(iface, "Tagged", |ty| matches!(ty, Ty::TypeVar(..)));
    assert!(matches!(row.origin, ExportedImplOrigin::OutOfBody));
    assert_eq!(row.generic_params.len(), 1);
    assert_eq!(row.generic_params[0].as_str(), "T");
    assert_eq!(
        row.param_bounds,
        [Vec::new()],
        "unbounded param exports an empty conjunction"
    );
    assert!(
        matches!(&row.for_ty_pattern, Ty::TypeVar(p, _) if p.as_str() == "T"),
        "a blanket impl's for-ty is the bare param, got {:?}",
        row.for_ty_pattern
    );
    let tag = impl_method(&row.methods, "tag");
    assert!(
        matches!(&tag.sig.params[0].ty, Ty::TypeVar(p, _) if p.as_str() == "T"),
        "the blanket receiver stays the impl's param, got {:?}",
        tag.sig.params[0].ty
    );
    assert!(matches!(&tag.sig.return_type, Ty::String { .. }));
}

#[test]
fn stdlib_impls_export_and_int_equals_is_complete() {
    let db = make_db();

    // Every stdlib package derives its impls table without panicking.
    for name in baml_builtins2::stdlib_package_names().iter().copied() {
        let _ = &package_interface(&db, PackageId::new(&db, Name::new(name))).impls;
    }

    // Spot-check `implement Equals for int` (baml.ops).
    let baml = package_interface(&db, PackageId::new(&db, Name::new("baml")));
    assert!(!baml.impls.is_empty(), "the stdlib exports impl rows");
    let row = baml
        .impls
        .iter()
        .find(|row| {
            row.interface.name.name().as_str() == "Equals"
                && *row.interface.name.namespace() == [Name::new("ops")]
                && matches!(row.for_ty_pattern, Ty::Int { .. })
        })
        .expect("baml.ops's `implement Equals for int` is exported");
    assert!(matches!(row.origin, ExportedImplOrigin::OutOfBody));
    assert!(row.generic_params.is_empty());
    assert!(row.interface.generics.is_empty());

    let eq = impl_method(&row.methods, "eq");
    assert!(
        matches!(&eq.sig.params[0].ty, Ty::Int { .. }),
        "self realizes to int"
    );
    assert!(
        matches!(&eq.sig.params[1].ty, Ty::Int { .. }),
        "`other: Self` realizes to int, got {:?}",
        eq.sig.params[1].ty
    );
    assert!(matches!(&eq.sig.return_type, Ty::Bool { .. }));
    assert!(matches!(&eq.sig.callable_throws, Ty::Never { .. }));
    assert_eq!(
        eq.sig.builtin_kind,
        Some(baml_compiler2_ast::BuiltinKind::Vm),
        "`$rust_function` bodies keep their builtin kind"
    );
}

// ── Mounted source-less packages in type positions ─────────────────────────

/// A package mounted as a `borsh(PackageInterface)`
/// blob — with NO source files — resolves in TYPE positions at check level.
/// Each test compiles a fixture "library" package (files under
/// `<builtin>/app/…` so `file_package` assigns them the package name `app`),
/// captures its interface blob, mounts it in a FRESH database as `app`, and
/// runs `collect_diagnostics` over consumer code. The call/runtime integration
/// suites exercise the same blobs beyond this check-level module.
pub(super) mod mounted {
    use baml_base::Name;
    use baml_compiler2_hir::package::PackageId;
    use baml_compiler2_tir::package_interface::package_interface;
    use baml_db::{ProjectDatabase, testing::assert_no_diagnostic_errors};

    use super::super::support::make_db;

    /// Compile `files` as the source of a library package named `app` and
    /// capture its `ArtifactKind::PackageInterface` envelope. The files ride
    /// under `<builtin>/app/…` so `file_package` assigns them the `app`
    /// package — the blob's qualified names therefore read `app.…`, matching
    /// the mount alias exactly (re-aliasing a blob under a different name is
    /// out of scope for this PR).
    pub(crate) fn app_blob(files: &[(&str, &str)]) -> Vec<u8> {
        let mut db = make_db();
        db.dependency("app");
        for (path, src) in files {
            // A source-bearing `Dependency` root at `<builtin>/app`: library
            // fixtures ride the same `<builtin>/` emit group the stdlib does.
            db.file(format!("<builtin>/app/{path}"), src);
        }
        assert_no_diagnostic_errors(&db);
        let iface = package_interface(&db, PackageId::new(&db, Name::new("app")));
        assert!(
            iface.types.values().any(|ns| !ns.is_empty()),
            "the library fixture must export at least one type"
        );
        baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, iface)
            .expect("serialize app interface")
    }

    /// A fresh consumer database with `blob` mounted as `app` and NO `app`
    /// source anywhere.
    fn consumer_db(blob: Vec<u8>, files: &[(&str, &str)]) -> ProjectDatabase {
        let mut db = make_db();
        db.set_mounted_packages([("app".to_string(), blob)].into())
            .unwrap();
        for (path, src) in files {
            db.file(path, src);
        }
        db
    }

    /// The check-level error messages (with codes) for user files of `db`.
    fn error_messages(db: &ProjectDatabase) -> Vec<String> {
        baml_db::collect_diagnostics(db)
            .iter()
            .filter(|d| matches!(d.severity, baml_compiler_diagnostics::Severity::Error))
            .map(|d| format!("[{}] {}", d.code(), d.message))
            .collect()
    }

    #[track_caller]
    fn assert_clean(db: &ProjectDatabase) {
        let errors = error_messages(db);
        assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
    }

    #[track_caller]
    fn assert_error_containing(db: &ProjectDatabase, needle: &str) {
        let errors = error_messages(db);
        assert!(
            errors.iter().any(|msg| msg.contains(needle)),
            "expected an error containing {needle:?}, got:\n{errors:#?}"
        );
    }

    /// The shared library fixture: a class implementing a library interface
    /// (in-body impl), blob-side `Equals`/`Compare` impls, an enum, an alias,
    /// and a free function.
    const LIB: &str = r#"
interface Describable {
    function describe(self) -> string throws never
}

class Widget {
    x int
    label string

    implements Describable {
        function describe(self) -> string throws never {
            self.label
        }
    }
}

implement baml.ops.Equals for Widget {
    function eq(self, other: Self) -> bool throws never {
        self.x == other.x
    }
}

implement baml.ops.Compare for Widget {
    function lt(self, other: Self) -> bool throws never {
        self.x < other.x
    }
}

enum Status {
    Active
    Retired
}

type Score = int

interface Taggable {
    function tag(self) -> string throws never
}

implement<T> Taggable for T {
    function tag(self) -> string throws never {
        "any"
    }
}

function add(a: int, b: int) -> int throws never {
    a + b
}
"#;

    fn lib_blob() -> Vec<u8> {
        app_blob(&[("lib.baml", LIB)])
    }

    #[test]
    fn class_resolves_in_type_position() {
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
function f() -> int throws never {
    let w: app.Widget[] = []
    let o: app.Widget? = null
    0
}
"#,
            )],
        );
        assert_clean(&db);
    }

    #[test]
    fn interface_annotation_and_bound_consult_blob_impls() {
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
function pick<T extends app.Describable>(a: T, b: T) -> T throws never {
    a
}

function g(w: app.Widget) -> app.Widget throws never {
    pick(w, w)
}

function h(w: app.Widget) -> int throws never {
    let d: app.Describable = w
    0
}
"#,
            )],
        );
        assert_clean(&db);
    }

    #[test]
    fn user_impl_of_mounted_interface_checks_conformance() {
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
class Card {
    title string

    implements app.Describable {
        function describe(self) -> string throws never {
            self.title
        }
    }
}

function f(c: Card) -> int throws never {
    let d: app.Describable = c
    0
}
"#,
            )],
        );
        assert_clean(&db);
    }

    #[test]
    fn user_impl_of_mounted_interface_missing_method_is_rejected() {
        // E0113: the blob row's REQUIRED method list drives coverage.
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
class Card {
    title string

    implements app.Describable {
    }
}
"#,
            )],
        );
        assert_error_containing(&db, "[E0113]");
        assert_error_containing(&db, "describe");
    }

    #[test]
    fn user_impl_of_mounted_interface_signature_mismatch_is_rejected() {
        // E0120: the override must conform to the blob row's symbolic-Self
        // signature.
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
class Card {
    title string

    implements app.Describable {
        function describe(self) -> int throws never {
            1
        }
    }
}
"#,
            )],
        );
        assert_error_containing(&db, "[E0120]");
        assert_error_containing(&db, "describe");
    }

    #[test]
    fn match_over_mounted_enum_is_exhaustiveness_checked() {
        let blob = lib_blob();
        let db = consumer_db(
            blob.clone(),
            &[(
                "main.baml",
                r#"
function m(s: app.Status) -> int throws never {
    match s {
        app.Status.Active => 1
        app.Status.Retired => 2
    }
}
"#,
            )],
        );
        assert_clean(&db);

        // A missing arm gets the normal non-exhaustive diagnostic, fed by the
        // blob's variant list.
        let db = consumer_db(
            blob,
            &[(
                "main.baml",
                r#"
function m(s: app.Status) -> int throws never {
    match s {
        app.Status.Active => 1
    }
}
"#,
            )],
        );
        assert_error_containing(&db, "Retired");
    }

    #[test]
    fn alias_from_blob_expands() {
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
function a(s: app.Score) -> int throws never {
    let t: app.Score = 5
    s + t
}
"#,
            )],
        );
        assert_clean(&db);
    }

    #[test]
    fn operators_resolve_through_blob_impl_rows() {
        // `==` is universal; `<` is the check-observable consumer of the blob's
        // `Equals`/`Compare` rows (ordering requires `baml.ops.Compare`
        // membership, discharged through the mounted impls table).
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
function cmp(a: app.Widget, b: app.Widget) -> bool throws never {
    let eq: bool = a == b
    a < b
}
"#,
            )],
        );
        assert_clean(&db);
    }

    #[test]
    fn absent_name_gets_normal_unresolved_diagnostic() {
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
function n() -> int throws never {
    let x: app.Nope = 1
    0
}
"#,
            )],
        );
        assert_error_containing(&db, "Nope");
    }

    #[test]
    fn function_reference_and_call_type_from_mounted_signature() {
        // References and direct calls share the exported signature and both
        // receive a loc-free External resolution.
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
function use_f(f: (a: int, b: int) -> int) -> int throws never {
    0
}

function r() -> int throws never {
    use_f(app.add) + app.add(1, 2)
}
"#,
            )],
        );
        assert_clean(&db);
    }

    #[test]
    fn user_impl_overlapping_mounted_blanket_impl_is_coherence_checked() {
        // The blob exports `implement<T> Taggable for T`. A user impl of
        // `app.Taggable` for a local class overlaps it — E0132, attributed
        // primary-only at the user's impl (the blob row has no span), with
        // the partner rendered structurally.
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r#"
class Mine {
    x int
}

implement app.Taggable for Mine {
    function tag(self) -> string throws never {
        "mine"
    }
}
"#,
            )],
        );
        assert_error_containing(&db, "overlapping interface implementations");
        assert_error_containing(&db, "mounted dependency");
    }

    #[test]
    fn stream_companion_of_mounted_class_resolves_in_consumer_expansion() {
        // Canonical PPIR `$stream` companions are exported as ordinary type
        // rows. A consumer-side declarative LLM expansion can therefore name
        // the mounted return type's companion with no dependency source.
        let db = consumer_db(
            lib_blob(),
            &[(
                "main.baml",
                r##"
client Dummy = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = "test",
);

function Ask() -> app.Widget {
    client Dummy
    prompt `Return a widget`
}
"##,
            )],
        );
        assert_clean(&db);
    }
}
