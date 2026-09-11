//! Test-only source roots, so unit tests can build compile-time heads
//! ([`DeclName`]) without a compiler database.
//!
//! One process-wide salsa database owns the roots. The common package
//! spellings are created up front in alphabetical order, so their ids — and
//! therefore the canonical order of heads across packages — match the wire's
//! name order that the tests were written against.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use baml_base::{LangPackage, LangRoots, Name, SourceRoot, SourceRootKind};

use crate::{DeclName, Ty, TyAttr, unify::AliasEquivCtx};

struct World {
    db: salsa::DatabaseImpl,
    roots: HashMap<String, SourceRoot>,
}

/// Spellings every test may use, created in this (alphabetical) order.
const PRELUDE: [&str; 8] = ["ai", "app", "baml", "dep", "me", "other", "reflect", "user"];

static WORLD: LazyLock<Mutex<World>> = LazyLock::new(|| {
    let mut world = World {
        db: salsa::DatabaseImpl::default(),
        roots: HashMap::new(),
    };
    for package in PRELUDE {
        create(&mut world, package);
    }
    Mutex::new(world)
});

fn create(world: &mut World, package: &str) -> SourceRoot {
    let World { db, roots } = world;
    let unnamed = package == "user";
    let root = SourceRoot::new(
        db,
        PathBuf::from(format!("<test>/{package}")),
        if unnamed {
            SourceRootKind::Workspace
        } else {
            SourceRootKind::Stdlib
        },
        (!unnamed).then(|| Name::new(package)),
        Vec::new(),
        None,
        Vec::new(),
    );
    roots.insert(package.to_string(), root);
    root
}

/// The test root spelled `package`, created on first use and stable for the
/// rest of the process.
pub(crate) fn root(package: &str) -> SourceRoot {
    let mut world = WORLD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(root) = world.roots.get(package) {
        return *root;
    }
    create(&mut world, package)
}

/// A head at the root namespace of the nameless workspace root — the
/// compile-time counterpart of the wire's `TypeName::local`.
pub(crate) fn local(name: Name) -> DeclName {
    DeclName::in_root(root("user"), Vec::new(), name)
}

/// The shape of the wire constructor `QualifiedTypeName::new(pkg, ns, name)`,
/// so a test written against it reads the same with a test root.
pub(crate) fn new(package: Name, namespace: Vec<Name>, name: Name) -> DeclName {
    DeclName::in_root(root(package.as_str()), namespace, name)
}

/// The language-root table over the test roots.
pub(crate) fn lang() -> LangRoots {
    LangPackage::ALL
        .into_iter()
        .fold(LangRoots::default(), |lang, package| {
            lang.with(package, root(package.manifest_name()))
        })
}

/// `Class(name)` at the nameless workspace root, no type args.
pub(crate) fn class(name: &str) -> Ty {
    Ty::Class(local(Name::new(name)), Box::new([]), TyAttr::default())
}

/// `Class(name, args)` at the nameless workspace root.
pub(crate) fn class_with_args(name: &str, args: Vec<Ty>) -> Ty {
    Ty::Class(local(Name::new(name)), args.into(), TyAttr::default())
}

/// The alias-equivalence context over `aliases` with the test language roots.
pub(crate) fn alias_ctx(aliases: &std::collections::HashMap<DeclName, Ty>) -> AliasEquivCtx<'_> {
    AliasEquivCtx {
        aliases,
        lang: lang(),
    }
}
