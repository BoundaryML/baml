//! BEP-066 slice 6a: the enriched `PackageInterface` export schema.
//!
//! These tests pin the *derivation* of the new loc-free rows — interface
//! surfaces (params/bounds/`requires`/associated types/fields/methods), class
//! field attributes and per-parameter bounds, function bounds and stable
//! fully-qualified names, and the full namespace set — plus borsh round-trip
//! and cross-database byte determinism. Nothing consumes these rows yet; the
//! resolution rewires land in follow-up PRs, and `cargo test -p baml_tests`
//! snapshots prove the derivation stays behavior-invisible.

use baml_base::Name;
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::{
    package_interface::{
        ExportedAssociatedType, ExportedFunction, ExportedType, PackageInterface, package_interface,
    },
    ty::Ty,
};
use baml_project::{ProjectDatabase, testing::assert_no_diagnostic_errors};

use super::support::make_db;

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
"#;

fn fixture_db() -> ProjectDatabase {
    let mut db = make_db();
    db.add_file("main.baml", FIXTURE);
    db.add_file(
        "ns_util/helpers.baml",
        "function helper() -> int throws never {\n    1\n}\n",
    );
    // A namespace whose ONLY item is an interface: before slice 6a its
    // namespace was invisible in the interface (interfaces were dropped and
    // nothing else exported from it).
    db.add_file("ns_shapes/only_iface.baml", "interface Marker {\n}\n");
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
        methods,
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

    // Required AND default methods, signatures with symbolic `Self`.
    let next = method(methods, "next");
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

    let twice = method(methods, "twice");
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
        methods,
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

    let next = method(methods, "next");
    assert_eq!(next.callable_fqn, "baml.iter.Iterator.next");
    assert!(matches!(&next.params[0].ty, Ty::TypeVar(p, _) if p.as_str() == "Self"));
    let map = method(methods, "map");
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
