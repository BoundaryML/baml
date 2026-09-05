use baml_type::{
    Name, ParamTy, QualifiedTypeName, Ty, TyAttr,
    unify::{AliasEquivCtx, TypeBindings, contains_bound_typevar},
};

fn match_ty_patterns(
    pairs: &[(&Ty, &Ty)],
    params: &[ParamTy],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    let mut bindings = rustc_hash::FxHashMap::default();
    for (pattern, target) in pairs {
        if !super::match_pattern(
            &super::interned_ty(pattern),
            &super::interned_ty(target),
            params,
            &mut bindings,
            &AliasEquivCtx(aliases),
        ) {
            return None;
        }
    }
    Some(
        bindings
            .into_iter()
            .map(|(param, ty)| (param, super::closed_plain(&ty)))
            .collect(),
    )
}

fn qtn(namespace: &[&str], name: &str) -> QualifiedTypeName {
    QualifiedTypeName::new(
        Name::new("user"),
        namespace.iter().map(|part| Name::new(*part)).collect(),
        Name::new(name),
    )
}

fn class(namespace: &[&str], name: &str, args: Vec<Ty>) -> Ty {
    Ty::Class(qtn(namespace, name), args.into(), TyAttr::default())
}

fn interface(name: &str, args: Vec<Ty>) -> Ty {
    Ty::Interface(qtn(&[], name), args.into(), Box::new([]), TyAttr::default())
}

fn int() -> Ty {
    Ty::Int {
        attr: TyAttr::default(),
    }
}

fn string() -> Ty {
    Ty::String {
        attr: TyAttr::default(),
    }
}

fn type_var(name: &str) -> Ty {
    Ty::TypeVar(param(name), TyAttr::default())
}

fn param(name: &str) -> ParamTy {
    ParamTy::new(0, Name::new(name))
}

#[test]
fn match_ty_pattern_rejects_repeated_type_var_conflict() {
    let pattern = class(&[], "Pair", vec![type_var("T"), type_var("T")]);
    let good = class(&[], "Pair", vec![int(), int()]);
    let bad = class(&[], "Pair", vec![int(), string()]);
    let params = vec![param("T")];

    assert!(
        match_ty_patterns(
            &[(&pattern, &good)],
            &params,
            &std::collections::HashMap::default()
        )
        .is_some()
    );
    assert!(
        match_ty_patterns(
            &[(&pattern, &bad)],
            &params,
            &std::collections::HashMap::default()
        )
        .is_none()
    );
}

#[test]
fn match_ty_pattern_matches_enum_variant_against_enum() {
    let side = Ty::Enum(qtn(&[], "Side"), TyAttr::default());
    let side_left = Ty::EnumVariant(qtn(&[], "Side"), Name::new("Left"), TyAttr::default());
    let other = Ty::EnumVariant(qtn(&[], "Coin"), Name::new("Heads"), TyAttr::default());
    let aliases = std::collections::HashMap::default();

    assert!(
        match_ty_patterns(&[(&side, &side_left)], &[], &aliases).is_some(),
        "`Side.Left` should match a `for Side` pattern",
    );
    assert!(
        match_ty_patterns(&[(&side, &other)], &[], &aliases).is_none(),
        "a variant of a *different* enum must not match",
    );
}

#[test]
fn match_ty_pattern_handles_nested_interface_args() {
    let pattern = interface(
        "Container",
        vec![Ty::List(Box::new(type_var("T")), TyAttr::default())],
    );
    let actual = interface(
        "Container",
        vec![Ty::List(Box::new(int()), TyAttr::default())],
    );
    let params = vec![param("T")];

    let bindings = match_ty_patterns(
        &[(&pattern, &actual)],
        &params,
        &std::collections::HashMap::default(),
    )
    .expect("nested list arg should bind T");
    assert_eq!(bindings.get(&param("T")), Some(&int()));
}

#[test]
fn contains_bound_typevar_checks_interface_associated_bindings() {
    let ty = Ty::Interface(
        qtn(&[], "Source"),
        Box::new([]),
        Box::new([(
            Name::new("Item"),
            Ty::List(Box::new(type_var("T")), TyAttr::default()),
        )]),
        TyAttr::default(),
    );

    assert!(contains_bound_typevar(&ty, &[param("T")]));
    assert!(!contains_bound_typevar(&ty, &[param("U")]));
}

#[test]
fn match_ty_pattern_uses_full_qualified_type_names() {
    let pattern = class(&["alpha"], "Thing", vec![]);
    let same_short_name = class(&["beta"], "Thing", vec![]);

    assert!(
        match_ty_patterns(
            &[(&pattern, &same_short_name)],
            &[],
            &std::collections::HashMap::default()
        )
        .is_none(),
        "same short name in different namespaces must not match"
    );
}

#[test]
fn match_ty_pattern_unions_are_order_insensitive_with_bindings() {
    let pattern = Ty::Union(Box::new([type_var("T"), string()]), TyAttr::default());
    let actual = Ty::Union(Box::new([string(), int()]), TyAttr::default());
    let params = vec![param("T")];

    let bindings = match_ty_patterns(
        &[(&pattern, &actual)],
        &params,
        &std::collections::HashMap::default(),
    )
    .expect("union members should be matched by type, not position");
    assert_eq!(bindings.get(&param("T")), Some(&int()));
}
fn package_qtn(pkg: &str, name: &str) -> QualifiedTypeName {
    QualifiedTypeName::new(Name::new(pkg), Vec::new(), Name::new(name))
}

#[test]
fn collect_ty_packages_covers_head_and_nested_covered_args() {
    let ty = Ty::Class(
        package_qtn("user", "Box"),
        Box::new([Ty::Enum(package_qtn("dep", "Meters"), TyAttr::default())]),
        TyAttr::default(),
    );
    let mut out = Vec::new();
    super::collect_packages(&super::interned_ty(&ty), &mut out);
    assert!(
        out.contains(&Name::new("user")),
        "for-type head package, got {out:?}"
    );
    assert!(
        out.contains(&Name::new("dep")),
        "nested covered-arg package, got {out:?}"
    );
}

#[test]
fn collect_interface_packages_covers_head_args_and_pins() {
    let iface = baml_type::Interface::new(
        package_qtn("ifacepkg", "Conv"),
        Box::new([Ty::Class(
            package_qtn("argpkg", "Meters"),
            Box::new([]),
            TyAttr::default(),
        )]),
        Box::new([(
            Name::new("Out"),
            Ty::Enum(package_qtn("pinpkg", "Unit"), TyAttr::default()),
        )]),
    );
    let mut out = Vec::new();
    super::collect_packages(&super::interned_ty(&iface.to_ty()), &mut out);
    for pkg in ["ifacepkg", "argpkg", "pinpkg"] {
        assert!(out.contains(&Name::new(pkg)), "missing {pkg}, got {out:?}");
    }
}
