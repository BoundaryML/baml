//! A program from its packages: every package of the program emitted on its
//! own ([`emit_package`]) — or served by a [`PackageCache`] that still holds
//! its current output — then bound and laid out by the linker.
//!
//! This is the whole of what knows about a "program" — the emitter compiles
//! one package against its dependencies' interfaces and the linker binds
//! outputs it is handed; neither sees the other. A driver that emits
//! packages in parallel across the dependency DAG changes only this file.

use baml_base::{FileId, Name, SourceFile, SourceRoot};
use baml_compiler_diagnostics::{Diagnostic, Severity};
use baml_compiler2_emit::{
    EmittedSubmission, LoweringError, OptLevel, emit_package, emit_session_submission,
    project_source_content_hash,
};
use baml_compiler2_hir::package::{edge_table, world_files, world_roots};
use baml_compiler2_hir_ty::render::Viewpoint;
use baml_linker::{LinkError, LinkPackage, LinkPackageId, LinkSet, link};
pub use baml_linker_types::EmittedPackage;
use bex_vm_types::Program;

use crate::{ProjectDatabase, check::ProjectDiagnostics};

/// Where a driver looks before emitting a package: a store of outputs
/// produced earlier for the same inputs. A package's output is a pure
/// function of its sources and its dependencies' interfaces, so a store that
/// keys on those may serve it in place of a fresh emit.
pub trait PackageCache {
    /// The output a previous compile of `root` at `opt` produced, if this
    /// store holds one for the package's current inputs.
    fn load(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        root: SourceRoot,
        opt: OptLevel,
    ) -> Option<EmittedPackage>;

    /// Offer the output a fresh emit of `root` at `opt` produced. Best-effort:
    /// a store that cannot keep it serves nothing for it next time.
    fn store(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        root: SourceRoot,
        opt: OptLevel,
        emitted: &EmittedPackage,
    );
}

/// The store that holds nothing: every package emits fresh.
pub struct NoCache;

impl PackageCache for NoCache {
    fn load(
        &self,
        _db: &dyn baml_compiler2_hir::Db,
        _root: SourceRoot,
        _opt: OptLevel,
    ) -> Option<EmittedPackage> {
        None
    }

    fn store(
        &self,
        _db: &dyn baml_compiler2_hir::Db,
        _root: SourceRoot,
        _opt: OptLevel,
        _emitted: &EmittedPackage,
    ) {
    }
}

/// A world - `root` with its dependency closure - that type-checked clean at
/// the database's current revision: this crate's only road to emit (the
/// emitter's own entry points are its lower layer, and code generation
/// probes what it lowers in debug builds). Holding the database by shared
/// reference, it cannot outlive an edit, so the program it compiles is the
/// program it checked.
///
/// The error predicate lives here and nowhere else: an error diagnostic
/// counts when its primary span is in a world file **or when it has no
/// span at all** - a span-less error is still an error of this check, and
/// gating on spans alone let it through. A refused world hands its errors
/// back ([`WorldHasErrors`]), so a caller renders this verdict instead of
/// re-deciding it.
#[must_use]
pub struct CheckedWorld<'db> {
    db: &'db ProjectDatabase,
    root: SourceRoot,
}

impl std::fmt::Debug for CheckedWorld<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckedWorld")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl<'db> CheckedWorld<'db> {
    /// Admits `root`'s world when `diagnostics`, collected from the database
    /// at this revision, carry no error of it.
    ///
    /// # Errors
    ///
    /// [`WorldHasErrors`] with the world's error diagnostics.
    pub fn check(
        diagnostics: &ProjectDiagnostics<'db>,
        root: SourceRoot,
    ) -> Result<Self, WorldHasErrors> {
        let db = diagnostics.db();
        let world: std::collections::HashSet<FileId> = world_files(db, root)
            .iter()
            .map(|file| file.file_id(db))
            .collect();
        let errors: Vec<Diagnostic> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .filter(|diagnostic| {
                diagnostic
                    .primary_span()
                    .is_none_or(|span| world.contains(&span.file_id))
            })
            .cloned()
            .collect();
        if !errors.is_empty() {
            return Err(WorldHasErrors { errors });
        }
        Ok(Self { db, root })
    }

    /// [`Self::check`] over freshly collected diagnostics.
    ///
    /// # Errors
    ///
    /// As [`Self::check`].
    pub fn collect(db: &'db ProjectDatabase, root: SourceRoot) -> Result<Self, WorldHasErrors> {
        Self::check(&ProjectDiagnostics::collect(db), root)
    }

    /// The database the world was checked in.
    pub fn db(&self) -> &'db ProjectDatabase {
        self.db
    }

    /// The package the world is rooted at: its program's viewpoint.
    pub fn root(&self) -> SourceRoot {
        self.root
    }

    /// The program of the world: every package emitted on its own and the
    /// outputs linked.
    ///
    /// # Errors
    ///
    /// [`CompileProgramError`]: a package that fails to emit, or outputs that
    /// fail to bind.
    pub fn compile(&self, opt: OptLevel) -> Result<Program, CompileProgramError> {
        self.compile_with(opt, &NoCache)
    }

    /// [`Self::compile`] with each package served from `cache` when it holds
    /// the package's current output, and offered to it when it does not.
    ///
    /// # Errors
    ///
    /// As [`Self::compile`]. A served output is trusted as the cache's own
    /// contract; one that does not bind fails the link like any other.
    pub fn compile_with(
        &self,
        opt: OptLevel,
        cache: &dyn PackageCache,
    ) -> Result<Program, CompileProgramError> {
        compile_program_with(self.db, self.root, opt, cache)
    }

    /// The root package's interface as an artifact carries it.
    pub fn export_interface(&self) -> baml_compiler2_hir_ty::package_interface::WireInterface {
        baml_compiler2_hir_ty::package_interface::export_interface(self.db, self.root)
    }

    /// The root package's own output, emitted against its dependencies'
    /// interfaces.
    ///
    /// # Errors
    ///
    /// [`LoweringError`]: the package failed to emit.
    pub fn emit_package(&self, opt: OptLevel) -> Result<EmittedPackage, LoweringError> {
        emit_package(self.db, self.root, opt)
    }

    /// The root package's output for a session submission, one of the root's
    /// own files.
    ///
    /// # Errors
    ///
    /// [`LoweringError`]: the package failed to emit.
    pub fn emit_session_submission(
        &self,
        submission: SourceFile,
        opt: OptLevel,
    ) -> Result<EmittedSubmission, LoweringError> {
        debug_assert!(
            self.root.files(self.db).contains(&submission),
            "a session submission is a file of the world's root"
        );
        emit_session_submission(self.db, self.root, submission, opt)
    }
}

/// Why [`CheckedWorld::check`] refused a world: its error diagnostics.
#[derive(Debug)]
pub struct WorldHasErrors {
    errors: Vec<Diagnostic>,
}

impl WorldHasErrors {
    /// The world's error diagnostics, in the collector's order. Never empty:
    /// a world is refused only for an error.
    pub fn errors(&self) -> &[Diagnostic] {
        &self.errors
    }

    /// One line a build status can show: the first error with where it is,
    /// and how many more there are. A surface without a diagnostics list
    /// for the world's other files (a playground showing its root's files)
    /// otherwise has nothing to point the user at.
    pub fn summary(&self, db: &ProjectDatabase) -> String {
        let (first, rest) = self
            .errors
            .split_first()
            .unwrap_or_else(|| unreachable!("a world is refused only for an error"));
        let mut sources = std::collections::HashMap::new();
        let mut file_paths = std::collections::HashMap::new();
        if let Some(span) = first.primary_span()
            && let Some(path) = db.file_id_to_path(span.file_id)
        {
            if let Some(file) = db.get_file(path) {
                sources.insert(span.file_id, file.text(db).clone());
            }
            file_paths.insert(span.file_id, path.clone());
        }
        let line = baml_compiler_diagnostics::render_diagnostic(
            first,
            &sources,
            &file_paths,
            &baml_compiler_diagnostics::RenderConfig::concise(),
        );
        match rest.len() {
            0 => line,
            1 => format!("{line} (and 1 more error)"),
            more => format!("{line} (and {more} more errors)"),
        }
    }
}

impl std::fmt::Display for WorldHasErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.errors.len();
        write!(
            f,
            "the program has {count} compilation error{}",
            if count == 1 { "" } else { "s" }
        )
    }
}

impl std::error::Error for WorldHasErrors {}

/// Why a program could not be built.
#[derive(Debug)]
pub enum CompileProgramError {
    /// The world did not check clean, so nothing was emitted.
    WorldHasErrors(WorldHasErrors),
    /// A package failed to emit.
    Emit(LoweringError),
    /// The packages failed to link.
    Link(LinkError),
    /// A package served from its interface — one with no source to emit —
    /// has no compiled output in the store, so nothing can stand in the
    /// program for it.
    ServedWithoutOutput { package: Name },
}

impl CompileProgramError {
    /// The message a build status shows: a refused world's first error and
    /// where it is (see [`WorldHasErrors::summary`]), any other failure as
    /// it displays.
    pub fn describe(&self, db: &ProjectDatabase) -> String {
        match self {
            Self::WorldHasErrors(refused) => refused.summary(db),
            Self::Emit(_) | Self::Link(_) | Self::ServedWithoutOutput { .. } => self.to_string(),
        }
    }
}

impl std::fmt::Display for CompileProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorldHasErrors(error) => write!(f, "{error}"),
            Self::Emit(error) => write!(f, "{error}"),
            Self::Link(error) => write!(f, "{error}"),
            Self::ServedWithoutOutput { package } => write!(
                f,
                "package `{package}` is served from its interface, but no store holds its \
                 compiled output"
            ),
        }
    }
}

impl std::error::Error for CompileProgramError {}

impl From<WorldHasErrors> for CompileProgramError {
    fn from(error: WorldHasErrors) -> Self {
        Self::WorldHasErrors(error)
    }
}

impl From<LoweringError> for CompileProgramError {
    fn from(error: LoweringError) -> Self {
        Self::Emit(error)
    }
}

impl From<LinkError> for CompileProgramError {
    fn from(error: LinkError) -> Self {
        Self::Link(error)
    }
}

/// The runnable image of `root`'s program: the root and its dependency
/// closure, each package emitted on its own and the outputs linked. Reached
/// only through a [`CheckedWorld`].
fn compile_program_with(
    db: &dyn baml_compiler2_emit::Db,
    root: SourceRoot,
    opt: OptLevel,
    cache: &dyn PackageCache,
) -> Result<Program, CompileProgramError> {
    // A dependency without files declares nothing itself. Served from its
    // interface (a mounted package), it is in the program through the output
    // the store holds for it — there is nothing to emit, and a store that
    // holds nothing is an error, never a package quietly missing from the
    // set with every import of it left dangling. Without an interface it is
    // an empty package, and nothing can import from it. The root is always
    // in the program — it is the viewpoint every host name resolves from —
    // even before it declares anything.
    let packages = world_roots(db, root)
        .iter()
        .copied()
        .filter_map(|package| {
            let emitted = match cache.load(db, package, opt) {
                Some(emitted) => emitted,
                None if package != root && package.files(db).is_empty() => {
                    return package.interface(db).is_some().then(|| {
                        Err(CompileProgramError::ServedWithoutOutput {
                            package: Viewpoint::user_facing(db, root).display_label(package),
                        })
                    });
                }
                None => match emit_package(db, package, opt) {
                    Ok(emitted) => {
                        cache.store(db, package, opt, &emitted);
                        emitted
                    }
                    Err(error) => return Some(Err(CompileProgramError::Emit(error))),
                },
            };
            Some(Ok(LinkedPackage {
                root: package,
                edges: edge_table(db, package).clone(),
                emitted,
            }))
        })
        .collect::<Result<Vec<_>, CompileProgramError>>()?;
    let packages = program_order(db, root, packages);
    let mut program = link(&link_set(db, &packages, root))?;
    program.source_content_hash = Some(project_source_content_hash(db, root));
    Ok(program)
}

/// The program's package order — a function of the world graph alone, so
/// one world links to one executable however its packages were installed
/// (a dependency from source and the same dependency mounted from its
/// interface sit in different source-root tables): the language packages
/// first, in table order (the stdlib's user-independent prefix of every
/// index space), then the root and every package it reaches, breadth-first,
/// a package's edges in name order. A package the walk does not reach (one
/// behind a dependency that emitted no output) follows in table order.
fn program_order(
    db: &dyn baml_compiler2_hir::Db,
    root: SourceRoot,
    packages: Vec<LinkedPackage>,
) -> Vec<LinkedPackage> {
    let is_stdlib =
        |package: &LinkedPackage| package.root.kind(db) == baml_base::SourceRootKind::Stdlib;
    let (mut ordered, mut rest): (Vec<_>, Vec<_>) = packages.into_iter().partition(is_stdlib);
    let mut queue = std::collections::VecDeque::from([root]);
    let mut seen = std::collections::HashSet::from([root]);
    while let Some(next) = queue.pop_front() {
        let Some(position) = rest.iter().position(|package| package.root == next) else {
            continue;
        };
        let package = rest.remove(position);
        let mut edges: Vec<&(Name, SourceRoot)> = package
            .edges
            .iter()
            .filter(|(_, target)| target.kind(db) != baml_base::SourceRootKind::Stdlib)
            .collect();
        edges.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (_, target) in edges {
            if seen.insert(*target) {
                queue.push_back(*target);
            }
        }
        ordered.push(package);
    }
    ordered.extend(rest);
    ordered
}

/// One package of the program, ready to link: its output beside the identity
/// and edges the link set binds it through.
struct LinkedPackage {
    root: SourceRoot,
    /// The package's edge table
    /// ([`edge_table`](baml_compiler2_hir::package::edge_table)): every
    /// package it reaches, by the name it reaches it under.
    edges: Vec<(Name, SourceRoot)>,
    emitted: EmittedPackage,
}

/// The link set of the program's packages: each in program order
/// ([`program_order`]) under its display label from the root's viewpoint —
/// its declared name, else the name its nearest dependent reaches it by, else
/// the stand-in ([`Viewpoint::display_package`]) — its edges resolved to set
/// positions (an edge to a package that emitted no output is dropped —
/// nothing imports from it).
fn link_set<'a>(
    db: &dyn baml_compiler2_hir::Db,
    packages: &'a [LinkedPackage],
    root: SourceRoot,
) -> LinkSet<'a> {
    let viewpoint = Viewpoint::user_facing(db, root);
    let lang = baml_compiler2_hir::package::lang_roots(db);
    let position = |root: SourceRoot| {
        packages
            .iter()
            .position(|package| package.root == root)
            .map(|index| LinkPackageId(u32::try_from(index).expect("link sets fit u32")))
    };
    LinkSet {
        packages: packages
            .iter()
            .map(|package| LinkPackage {
                name: viewpoint.display_label(package.root),
                // The language packages are the prelude: reached under fixed
                // names by every package, declared by none. Which one a root
                // is, the installation table says; the edges to them are
                // edges like any other.
                kind: match lang.package_of(package.root) {
                    Some(language) => bex_vm_types::types::PackageKind::Language(language),
                    None => {
                        debug_assert_ne!(
                            package.root.kind(db),
                            baml_base::SourceRootKind::Stdlib,
                            "a stdlib root is installed as a language package"
                        );
                        bex_vm_types::types::PackageKind::Ordinary
                    }
                },
                edges: package
                    .edges
                    .iter()
                    .filter_map(|(edge, target)| {
                        Some(baml_linker::LinkEdge {
                            name: edge.clone(),
                            target: position(*target)?,
                            kind: bex_vm_types::types::EdgeKind::Dependency,
                        })
                    })
                    .collect(),
                unit: &package.emitted.unit,
                record: &package.emitted.record,
                tail: package.emitted.tail.as_ref(),
            })
            .collect(),
        root: position(root).unwrap_or_else(|| unreachable!("the root is always in the program")),
    }
}

#[cfg(test)]
mod tests {
    use baml_base::SourceRootKind;
    use baml_compiler_diagnostics::DiagnosticId;

    use super::*;
    use crate::SourceRootSpec;

    fn clean_world_db() -> (ProjectDatabase, SourceRoot) {
        let mut db = ProjectDatabase::new();
        let root = db
            .add_source_root(SourceRootSpec::new(".", SourceRootKind::Workspace))
            .unwrap_or_else(|err| unreachable!("a fresh database has no workspace root: {err}"));
        db.add_or_update_file_in(
            root,
            std::path::Path::new("main.baml"),
            "function f() -> int { 0 }",
        );
        (db, root)
    }

    /// An error with no span belongs to no file, and still refuses the world.
    #[test]
    fn a_span_less_error_refuses_the_world() {
        let (db, root) = clean_world_db();
        let diagnostics = ProjectDiagnostics::from_parts_for_test(
            &db,
            vec![Diagnostic::error(DiagnosticId::TypeMismatch, "span-less")],
        );
        let refused = CheckedWorld::check(&diagnostics, root).expect_err("the error counts");
        assert_eq!(refused.errors().len(), 1);
    }

    /// The control: a span-less warning is no error.
    #[test]
    fn a_span_less_warning_admits_the_world() {
        let (db, root) = clean_world_db();
        let diagnostics = ProjectDiagnostics::from_parts_for_test(
            &db,
            vec![Diagnostic::warning(DiagnosticId::TypeMismatch, "span-less")],
        );
        assert!(CheckedWorld::check(&diagnostics, root).is_ok());
    }
}
