//! Test-only heads and viewpoints: roots minted in a minimal compiler
//! database, so the crate's pure unit tests (exhaustiveness, coherence,
//! unification) can build root-headed types and spell them without a project
//! database.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use baml_base::{LangPackage, LangRoots, Name, SourceRoot, SourceRootKind, SourceRootTable};
use baml_type::{DeclName, Ty, unify::AliasEquivCtx};

use crate::render::Viewpoint;

/// The smallest database the viewpoint and the package queries accept: a
/// root table and nothing else (no stdlib installed, no seeds). Cloning
/// yields another handle on the same storage.
#[salsa::db]
#[derive(Clone)]
struct TestDb {
    storage: salsa::Storage<Self>,
    roots: Option<SourceRootTable>,
}

#[salsa::db]
impl salsa::Database for TestDb {}

#[salsa::db]
impl baml_compiler2_hir::Db for TestDb {
    fn source_roots(&self) -> SourceRootTable {
        self.roots.expect("table present from construction")
    }
}

struct World {
    /// A database handle is not `Sync` (its query stack is thread-local), so
    /// the shared one lives behind a lock and each test thread works on a
    /// clone ([`db`]). Nothing is ever mutated after construction.
    db: Mutex<TestDb>,
    roots: HashMap<&'static str, SourceRoot>,
}

/// Every package a test may spell. The world is built once and never
/// mutated afterwards, so viewpoints can borrow its database for `'static`;
/// a test naming a package outside this list is told to add it here.
const PACKAGES: [&str; 22] = [
    "ai",
    "anthropic",
    "app",
    "argpkg",
    "assert",
    "aws",
    "baml",
    "claude_code",
    "dep",
    "google",
    "ifacepkg",
    "log",
    "me",
    "openai",
    "other",
    "pinpkg",
    "reflect",
    "testing",
    "trace",
    "typesafeai",
    "user",
    "vercel",
];

static WORLD: LazyLock<World> = LazyLock::new(|| {
    let mut db = TestDb {
        storage: salsa::Storage::default(),
        roots: None,
    };
    let mut roots = HashMap::new();
    let mut table = Vec::new();
    // Created in the list's alphabetical order, so root ids — and the canonical
    // order of heads across packages — match the name order the tests were
    // written against.
    for package in PACKAGES {
        let unnamed = package == "user";
        let root = SourceRoot::new(
            &db,
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
        roots.insert(package, root);
        table.push(root);
    }
    db.roots = Some(SourceRootTable::new(&db, table));
    World {
        db: Mutex::new(db),
        roots,
    }
});

thread_local! {
    /// This thread's handle on the world's database, leaked so a viewpoint
    /// over it can be `'static` (one handle per test thread, for the life of
    /// the test process).
    static DB: &'static TestDb = Box::leak(Box::new(
        WORLD
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone(),
    ));
}

/// This thread's handle on the test database.
fn db() -> &'static TestDb {
    DB.with(|db| *db)
}

/// The test root spelled `package`.
pub(crate) fn root(package: &str) -> SourceRoot {
    WORLD.roots.get(package).copied().unwrap_or_else(|| {
        panic!("test package `{package}` is not in `test_heads::PACKAGES`; add it there")
    })
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
    Ty::Class(local(Name::new(name)), Box::new([]))
}

/// `Class(name, args)` at the nameless workspace root.
pub(crate) fn class_with_args(name: &str, args: Vec<Ty>) -> Ty {
    Ty::Class(local(Name::new(name)), args.into())
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

/// The viewpoint diagnostics render from: user-facing for the nameless
/// workspace root (its own items bare, every other package spelled).
pub(crate) fn viewpoint() -> Viewpoint<'static> {
    Viewpoint::user_facing(db(), root("user"))
}
