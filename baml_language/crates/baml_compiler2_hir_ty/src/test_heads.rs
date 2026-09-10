//! Test-only heads and viewpoints: roots minted in a bare salsa database, so
//! the crate's pure unit tests (exhaustiveness, coherence, unification) can
//! build root-headed types and spell them without a compiler database.
//!
//! The common spellings are created in alphabetical order, so their ids — and
//! the canonical order of heads across packages — match the name order the
//! tests were written against.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use baml_base::{LangPackage, LangRoots, Name, SourceRoot, SourceRootKind};
use baml_compiler2_hir::package::Spelling;
use baml_type::{DeclName, Ty, TyAttr, unify::AliasEquivCtx};

use crate::render::Viewpoint;

struct World {
    db: salsa::DatabaseImpl,
    roots: HashMap<String, SourceRoot>,
    /// The spelling table over `roots`, rebuilt (and leaked, so viewpoints
    /// can be `'static`) after any root is created.
    spelling: Option<&'static Spelling>,
}

const PRELUDE: [&str; 8] = ["ai", "app", "baml", "dep", "me", "other", "reflect", "user"];

static WORLD: LazyLock<Mutex<World>> = LazyLock::new(|| {
    let mut world = World {
        db: salsa::DatabaseImpl::default(),
        roots: HashMap::new(),
        spelling: None,
    };
    for package in PRELUDE {
        create(&mut world, package);
    }
    Mutex::new(world)
});

fn create(world: &mut World, package: &str) -> SourceRoot {
    let World {
        db,
        roots,
        spelling,
    } = world;
    *spelling = None;
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

/// The test root spelled `package`, created on first use.
pub(crate) fn root(package: &str) -> SourceRoot {
    let mut world = WORLD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(root) = world.roots.get(package) {
        return *root;
    }
    create(&mut world, package)
}

/// The shape of the wire constructor `QualifiedTypeName::new(pkg, ns, name)`.
#[expect(
    clippy::needless_pass_by_value,
    reason = "mirrors the wire constructor's signature so tests read the same"
)]
pub(crate) fn new(package: Name, namespace: Vec<Name>, name: Name) -> DeclName {
    DeclName::in_root(root(package.as_str()), namespace, name)
}

/// A head at the root namespace of the nameless workspace root.
pub(crate) fn local(name: Name) -> DeclName {
    DeclName::in_root(root("user"), Vec::new(), name)
}

/// `Class(name)` at the nameless workspace root, no type args.
pub(crate) fn class(name: &str) -> Ty {
    Ty::Class(local(Name::new(name)), Box::new([]), TyAttr::default())
}

/// `Class(name, args)` at the nameless workspace root.
pub(crate) fn class_with_args(name: &str, args: Vec<Ty>) -> Ty {
    Ty::Class(local(Name::new(name)), args.into(), TyAttr::default())
}

/// The language-root table over the test roots.
pub(crate) fn lang() -> LangRoots {
    LangPackage::ALL
        .into_iter()
        .fold(LangRoots::default(), |lang, package| {
            lang.with(package, root(package.manifest_name()))
        })
}

/// The alias-equivalence context over `aliases` with the test language roots.
pub(crate) fn alias_ctx(aliases: &dyn baml_type::unify::AliasMap) -> AliasEquivCtx<'_> {
    AliasEquivCtx {
        aliases,
        lang: lang(),
    }
}

static NO_ALIASES: LazyLock<HashMap<DeclName, Ty>> = LazyLock::new(HashMap::new);

/// The alias-equivalence context with no aliases.
pub(crate) fn no_aliases() -> AliasEquivCtx<'static> {
    alias_ctx(&*NO_ALIASES)
}

/// The spelling table over every test root created so far.
pub(crate) fn spelling() -> &'static Spelling {
    let mut world = WORLD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(spelling) = world.spelling {
        return spelling;
    }
    let mut pairs: Vec<(SourceRoot, Name)> = world
        .roots
        .iter()
        .map(|(name, root)| (*root, Name::new(name)))
        .collect();
    pairs.sort();
    let spelling: &'static Spelling = Box::leak(Box::new(Spelling::from_pairs(pairs)));
    world.spelling = Some(spelling);
    spelling
}

/// The viewpoint diagnostics render from: user-facing for the nameless
/// workspace root (its own items bare, every other package spelled).
pub(crate) fn viewpoint() -> Viewpoint<'static> {
    let viewer = root("user");
    Viewpoint::over(spelling(), Some(viewer))
}
