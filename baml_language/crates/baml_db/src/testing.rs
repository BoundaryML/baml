//! Test helpers for compiling BAML source into bytecode.
//!
//! These utilities panic on compile errors, so they are appropriate for test
//! code only. Production callers check through [`crate::CheckedWorld`] and
//! render a refused world's errors.
//!
//! # These are for white-box tests, not behavior tests
//!
//! Reach for these only when the *subject* of the test is something Rust can
//! see but BAML cannot: emitted bytecode, VM/heap state, salsa invalidation,
//! diagnostics, or a host-boundary value.
//!
//! If the test just compiles some BAML, runs a function, and asserts the
//! returned value or a catchable throw, it does not belong in Rust at all —
//! write it as a `test` block in `crates/baml_tests/baml_src/`, where the whole
//! corpus compiles once instead of once per test. See
//! `baml_language/TEST_INSTRUCTIONS.md` ("Where does a new test go?").
//!
//! # Two flavors: honest and prefix-accelerated
//!
//! The stdlib is the same ~50 files on every compile, so re-deriving it per
//! test is pure waste — and under `cargo nextest`, which runs each test in its
//! own process, no in-process cache can amortize it. The fix is a
//! [`StdlibPrefix`]: the stdlib's typed interfaces plus each of its packages'
//! compiled output, built once per toolchain by a build script (see
//! `baml_tests::stdlib_prefix`) and handed to the compile helpers.
//!
//! - [`compile_source`] and friends derive everything honestly. They are the
//!   reference implementation, and the control arm the equivalence oracle in
//!   `baml_tests` compares the fast path against. They are not deprecated.
//! - [`compile_source_with_prefix`] and friends link the prefix's stdlib
//!   packages in. The output is **byte-identical** to the honest path (pinned
//!   by that oracle) because the stdlib *sources* stay in the database — only
//!   its interface derivation and its packages' emit are skipped.
//!
//! Keeping the sources is load-bearing, not incidental. A database that mounts
//! the stdlib as a source-less precompiled package (what
//! [`ProjectDatabase::ensure_precompiled_stdlib`] builds, and what
//! runtime `reflect.Package.compile` uses) is much faster still, but it is
//! not a faithful substitute: with no stdlib bodies to look through, a direct
//! sysop call lowers to a plain `call` instead of `sys_op`, and checks that
//! walk stdlib bodies or declaration sites go quiet. Emit helpers therefore
//! never use that mode.

use std::path::Path;

use baml_base::{Name, SourceFile, SourceRoot, SourceRootKind};
use baml_compiler_diagnostics::{Diagnostic, Severity};
pub use baml_compiler2_emit::OptLevel;
use bex_vm_types::Program;

use crate::{
    ProjectDatabase, SourceRootSpec,
    check::{ProjectDiagnostics, collect_compiler2_diagnostics_narrowed},
    collect_diagnostics,
    program::CheckedWorld,
    stdlib_prefix::StdlibPrefix,
};

/// A fresh database with the stdlib installed and one empty, unnamed
/// `Workspace` root (path `.`).
fn workspace_db() -> (ProjectDatabase, SourceRoot) {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(SourceRootSpec::new(".", SourceRootKind::Workspace))
        .unwrap_or_else(|err| unreachable!("a fresh database has no workspace root: {err}"));
    (db, root)
}

/// Set up a test database from BAML source code: the stdlib, a `Workspace`
/// root, and `source` as `test.baml` in it.
pub fn setup_test_db(source: &str) -> ProjectDatabase {
    let (mut db, root) = workspace_db();
    db.add_or_update_file_in(root, Path::new("test.baml"), source);
    db
}

/// Assert that the test's own sources check: no error diagnostic in a
/// workspace file, and none without a span.
#[track_caller]
pub fn assert_no_diagnostic_errors(db: &ProjectDatabase) {
    let diagnostics = collect_diagnostics(db);

    let user_file_ids: std::collections::HashSet<_> =
        db.workspace_files().iter().map(|f| f.file_id(db)).collect();

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .is_none_or(|span| user_file_ids.contains(&span.file_id))
        })
        .collect();
    assert_clean(&errors);
}

/// The world `root` heads, admitted by its check; a refused world panics with
/// every error rendered.
#[track_caller]
fn checked_world<'db>(
    diagnostics: &ProjectDiagnostics<'db>,
    root: SourceRoot,
) -> CheckedWorld<'db> {
    CheckedWorld::check(diagnostics, root)
        .unwrap_or_else(|refused| panic!("{}", errors_message(refused.errors())))
}

/// Compile BAML source with default optimization (`OptLevel::One`).
pub fn compile_source(source: &str) -> Program {
    compile_source_with_opt(source, OptLevel::One)
}

/// Compile BAML source with a specific optimization level.
pub fn compile_source_with_opt(source: &str, opt: OptLevel) -> Program {
    let (mut db, root) = workspace_db();
    db.add_or_update_file_in(root, Path::new("test.baml"), source);
    checked_world(&ProjectDiagnostics::collect(&db), root)
        .compile(opt)
        .expect("compile should succeed for valid test source")
}

/// Set up a test database whose stdlib interface derivation is served from
/// `prefix` instead of re-derived from source.
///
/// The stdlib *sources* stay in the database, so name resolution, sysop
/// recognition and every body-walking check behave exactly as they do without
/// a prefix — only the per-package `PackageInterface` derivation is skipped.
pub fn setup_test_db_with_prefix(prefix: &StdlibPrefix, source: &str) -> ProjectDatabase {
    let (mut db, root) = workspace_db_with_prefix(prefix);
    db.add_or_update_file_in(root, Path::new("test.baml"), source);
    db
}

/// [`workspace_db`] with the stdlib interface derivation served from
/// `prefix`. The stdlib sources are still installed (see the module docs on
/// why that is load-bearing); only the per-package `PackageInterface`
/// derivation reads the seed.
fn workspace_db_with_prefix(prefix: &StdlibPrefix) -> (ProjectDatabase, SourceRoot) {
    let (mut db, root) = workspace_db();
    db.set_seeded_stdlib_interface(prefix.interfaces.clone());
    (db, root)
}

/// [`setup_test_db_with_prefix`] for a project of several files, for tests
/// that need cross-file or namespaced (`ns_<name>/`) layout.
pub fn setup_multi_file_db_with_prefix(
    prefix: &StdlibPrefix,
    files: &[(&str, &str)],
) -> ProjectDatabase {
    let (mut db, root) = workspace_db_with_prefix(prefix);
    db.add_or_update_files_in(
        root,
        files
            .iter()
            .map(|(path, content)| (Path::new(*path), *content)),
    );
    db
}

/// Diagnostics for the project's **user** files only.
///
/// [`collect_diagnostics`] deliberately re-checks every file on every call,
/// because narrowing the set on a long-lived, edited database could serve stale
/// diagnostics for a clean file that depends on a changed signature. A test
/// database is neither long-lived nor edited: it is populated once and read
/// once, so nothing can be stale and the stdlib's ~50 files — which no test
/// asserts on, and which the callers below already filter out — need not be
/// checked at all.
///
/// The package-level pass runs too, so cross-file duplicate and shadow
/// diagnostics (which belong to no single file) are not lost.
pub fn check_user_files(db: &ProjectDatabase) -> Vec<Diagnostic> {
    let files = db.workspace_files();
    let mut diagnostics: Vec<Diagnostic> =
        files.iter().flat_map(|file| db.check_file(*file)).collect();
    diagnostics.extend(crate::check::package_level_diagnostics(db, &files));
    // Same total order [`collect_diagnostics`] imposes, so a caller that
    // snapshots the list sees no difference from narrowing the checked set.
    crate::check::sort_diagnostics(&mut diagnostics);
    diagnostics
}

/// Panic with every error rendered, or return if there are none.
#[track_caller]
fn assert_clean(errors: &[&Diagnostic]) {
    assert!(
        errors.is_empty(),
        "{}",
        errors_message(errors.iter().copied())
    );
}

/// Every error, one numbered line each.
fn errors_message<'a>(errors: impl IntoIterator<Item = &'a Diagnostic>) -> String {
    use std::fmt::Write;

    let mut msg = String::from("Compilation produced diagnostic errors:\n");
    for (i, err) in errors.into_iter().enumerate() {
        let _ = writeln!(
            msg,
            "  {}. [{}] {}",
            i + 1,
            err.code(),
            err.message_with_primary_label()
        );
    }
    msg
}

/// [`assert_no_diagnostic_errors`] over [`check_user_files`].
#[track_caller]
pub fn assert_no_user_diagnostic_errors(db: &ProjectDatabase) {
    let diagnostics = check_user_files(db);
    let errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert_clean(&errors);
}

/// [`compile_source_with_opt`] accelerated by `prefix`.
///
/// Byte-identical to the honest path — pinned by the equivalence oracle in
/// `baml_tests` — because the prefix only replaces work whose inputs are the
/// stdlib sources that are still present in the database.
///
/// # Panics
///
/// If `opt` differs from the level `prefix` was built at: the stdlib packages
/// it serves and the user package linked with them must be lowered the same
/// way.
pub fn compile_source_with_prefix(prefix: &StdlibPrefix, source: &str, opt: OptLevel) -> Program {
    compile_multi_file_with_prefix(prefix, &[("test.baml", source)], opt)
}

/// [`compile_multi_file`] accelerated by `prefix`. See
/// [`compile_source_with_prefix`] for the guarantees and the panic.
pub fn compile_multi_file_with_prefix(
    prefix: &StdlibPrefix,
    files: &[(&str, &str)],
    opt: OptLevel,
) -> Program {
    assert_eq!(
        prefix.opt, opt,
        "stdlib prefix was lowered at {:?} but the caller asked to compile at {opt:?}; \
         the served stdlib packages and the user package linked with them must agree",
        prefix.opt
    );
    let (mut db, root) = workspace_db_with_prefix(prefix);
    db.add_or_update_files_in(
        root,
        files
            .iter()
            .map(|(path, content)| (Path::new(*path), *content)),
    );
    // The stdlib's check is the prefix's: `build_stdlib_prefix` emits only a
    // stdlib that checked clean, so only the test's own files are checked.
    let user_files: std::collections::HashSet<SourceFile> =
        db.workspace_files().into_iter().collect();
    let diagnostics =
        collect_compiler2_diagnostics_narrowed(&db, &|file| user_files.contains(&file), Vec::new());
    checked_world(&diagnostics.merged, root)
        .compile_with(opt, prefix)
        .expect("compile should succeed for valid test source")
}

/// Compile multiple BAML files at the given relative paths in one project.
/// Use when a test needs cross-file or namespaced (`ns_<name>/`) layout,
/// which `compile_source`'s single-file helper can't express.
pub fn compile_multi_file(files: &[(&str, &str)]) -> Program {
    let (mut db, root) = workspace_db();
    db.add_or_update_files_in(
        root,
        files
            .iter()
            .map(|(path, content)| (Path::new(*path), *content)),
    );
    checked_world(&ProjectDiagnostics::collect(&db), root)
        .compile(OptLevel::One)
        .expect("compile should succeed for valid test source")
}

// ── Root-aware fixture builders ─────────────────────────────────────────────

/// Test-database conveniences over [`ProjectDatabase`]'s source-root API.
///
/// Test fixtures overwhelmingly want one thing: a stdlib-equipped database
/// with a single `Workspace` root that files are dropped into by path, plus
/// the occasional source-bearing dependency package. This trait spells that
/// out once so fixtures read `db.workspace(root)` / `db.file(path, text)`
/// instead of repeating the root bookkeeping. It is test support only —
/// production code adds roots and files explicitly.
pub trait TestDbExt {
    /// Install the stdlib sources (if not yet installed) and add an unnamed
    /// `Workspace` root at `root` — the package the default spelling `user`
    /// displays.
    ///
    /// # Panics
    ///
    /// Panics if `root` is rejected (see [`crate::SourceRootError`]), or if
    /// a non-stdlib root was added before the stdlib.
    fn workspace(&mut self, root: &Path) -> SourceRoot;

    /// Add a source-bearing `Dependency` root for `package` at
    /// `<builtin>/<package>`.
    ///
    /// The `<builtin>/` prefix is deliberate: it is the emit layer's wire
    /// contract for non-workspace units, so files added under this root
    /// (`db.file("<builtin>/<package>/lib.baml", ...)`) ride the same emit
    /// group as the stdlib and can be captured as a mountable
    /// `PackageInterface` blob plus symbolic compilation units.
    ///
    /// # Panics
    ///
    /// Panics if `package` is already served by another root or a mounted
    /// blob (see [`crate::SourceRootError`]).
    fn dependency(&mut self, package: &str) -> SourceRoot;

    /// Mount a serialized `PackageInterface` as a source-less package reached
    /// from the workspace root under `alias` — the runtime compiler's shape:
    /// a `Dynamic` root at `<builtin>/<alias>` served from the blob, plus the
    /// edge. Errors are the database's own ([`crate::SourceRootError`]).
    fn try_mount(
        &mut self,
        alias: &str,
        blob: Vec<u8>,
    ) -> Result<SourceRoot, crate::SourceRootError>;

    /// [`Self::try_mount`], panicking on refusal.
    fn mount(&mut self, alias: &str, blob: Vec<u8>) -> SourceRoot {
        self.try_mount(alias, blob)
            .unwrap_or_else(|err| panic!("cannot mount `{alias}`: {err}"))
    }

    /// The package the workspace root reaches by the edge `name` — a
    /// dependency added by [`Self::dependency`], a mount by [`Self::mount`],
    /// or a language package under its fixed name (`"baml"`, `"reflect"`),
    /// which every workspace root carries as edges. Names live on edges:
    /// this reads the workspace's.
    ///
    /// # Panics
    ///
    /// Panics without a workspace root, or when no edge of it is called
    /// `name`.
    fn dependency_named(&self, name: &str) -> SourceRoot;

    /// Add or update the file at `path`, owned by the live root whose path is
    /// the longest prefix of `path`; a path under no root (e.g. the bare
    /// `test.baml` most fixtures use) goes to the `Workspace` root.
    ///
    /// # Panics
    ///
    /// Panics if `path` is under no root and the database has no `Workspace`
    /// root — call [`TestDbExt::workspace`] first.
    fn file(&mut self, path: impl AsRef<Path>, text: &str) -> SourceFile;
}

impl TestDbExt for ProjectDatabase {
    fn workspace(&mut self, root: &Path) -> SourceRoot {
        self.ensure_stdlib_sources();
        self.add_source_root(SourceRootSpec::new(root, SourceRootKind::Workspace))
            .unwrap_or_else(|err| {
                panic!(
                    "cannot add the workspace source root at `{}`: {err}",
                    root.display()
                )
            })
    }

    fn dependency(&mut self, package: &str) -> SourceRoot {
        let workspace = self.workspace_root().unwrap_or_else(|| {
            panic!("`TestDbExt::dependency(\"{package}\")` needs a workspace root to depend on it")
        });
        let root = self
            .add_source_root(
                SourceRootSpec::new(format!("<builtin>/{package}"), SourceRootKind::Dependency)
                    .named(Name::new(package)),
            )
            .unwrap_or_else(|err| {
                panic!("cannot add the dependency source root `{package}`: {err}")
            });
        self.add_dependency(
            workspace,
            baml_base::Dependency {
                name: Name::new(package),
                root,
            },
        )
        .unwrap_or_else(|err| panic!("cannot depend on `{package}`: {err}"));
        root
    }

    fn dependency_named(&self, name: &str) -> SourceRoot {
        let workspace = self.workspace_root().unwrap_or_else(|| {
            panic!("`TestDbExt::dependency_named(\"{name}\")` needs a workspace root whose edge to read")
        });
        baml_compiler2_hir::package::dependency_named(self, workspace, &Name::new(name))
            .unwrap_or_else(|| panic!("the workspace reaches no package as `{name}`"))
    }

    fn try_mount(
        &mut self,
        alias: &str,
        blob: Vec<u8>,
    ) -> Result<SourceRoot, crate::SourceRootError> {
        let workspace = self.workspace_root().unwrap_or_else(|| {
            panic!("`TestDbExt::mount(\"{alias}\")` needs a workspace root to mount into")
        });
        let root = self.add_source_root(
            SourceRootSpec::new(format!("<builtin>/{alias}"), SourceRootKind::Dynamic)
                .named(Name::new(alias))
                .served_from(blob),
        )?;
        self.add_dependency(
            workspace,
            baml_base::Dependency {
                name: Name::new(alias),
                root,
            },
        )?;
        Ok(root)
    }

    fn file(&mut self, path: impl AsRef<Path>, text: &str) -> SourceFile {
        let path = path.as_ref();
        let root = self
            .source_root_for_path(path)
            .or_else(|| self.workspace_root())
            .unwrap_or_else(|| {
                panic!(
                    "no source root owns `{}` and the database has no workspace root; \
                     call `TestDbExt::workspace` (or `db_with_root`) first",
                    path.display()
                )
            });
        self.add_or_update_file_in(root, path, text)
    }
}
