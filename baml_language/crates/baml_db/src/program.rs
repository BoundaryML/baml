//! A program from its packages: every package of the program emitted on its
//! own ([`emit_package`]) — or served by a [`PackageCache`] that still holds
//! its current output — then bound and laid out by the linker.
//!
//! This is the whole of what knows about a "program" — the emitter compiles
//! one package against its dependencies' interfaces and the linker binds
//! outputs it is handed; neither sees the other. A driver that emits
//! packages in parallel across the dependency DAG changes only this file.

use baml_base::{Name, SourceRoot, SourceRootKind};
use baml_compiler2_emit::{LoweringError, OptLevel, emit_package, project_source_content_hash};
use baml_compiler2_hir::package::{edge_table, spelling, world_roots};
use baml_linker::{LinkError, LinkGroup, LinkPackage, LinkPackageId, LinkSet, link};
use baml_linker_types::EmittedPackage;
use bex_vm_types::Program;

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

/// Why a program could not be built.
#[derive(Debug)]
pub enum CompileProgramError {
    /// A package failed to emit.
    Emit(LoweringError),
    /// The packages failed to link.
    Link(LinkError),
}

impl std::fmt::Display for CompileProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Emit(error) => write!(f, "{error}"),
            Self::Link(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CompileProgramError {}

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
/// closure, each package emitted on its own and the outputs linked.
///
/// # Errors
///
/// [`CompileProgramError`]: a package that fails to emit, or outputs that
/// fail to bind.
pub fn compile_program(
    db: &dyn baml_compiler2_emit::Db,
    root: SourceRoot,
    opt: OptLevel,
) -> Result<Program, CompileProgramError> {
    compile_program_with(db, root, opt, &NoCache)
}

/// [`compile_program`] with each package served from `cache` when it holds
/// the package's current output, and offered to it when it does not.
///
/// # Errors
///
/// As [`compile_program`]. A served output is trusted as the cache's own
/// contract; one that does not bind fails the link like any other.
pub fn compile_program_with(
    db: &dyn baml_compiler2_emit::Db,
    root: SourceRoot,
    opt: OptLevel,
    cache: &dyn PackageCache,
) -> Result<Program, CompileProgramError> {
    // A root without files declares nothing: no output, and nothing to import
    // from it.
    let packages = world_roots(db, root)
        .iter()
        .copied()
        .filter(|root| !root.files(db).is_empty())
        .map(|root| {
            let emitted = match cache.load(db, root, opt) {
                Some(emitted) => emitted,
                None => {
                    let emitted = emit_package(db, root, opt)?;
                    cache.store(db, root, opt, &emitted);
                    emitted
                }
            };
            Ok(LinkedPackage {
                root,
                edges: edge_table(db, root),
                emitted,
            })
        })
        .collect::<Result<Vec<_>, LoweringError>>()?;
    let mut program = link(&link_set(db, &packages))?;
    program.source_content_hash = Some(project_source_content_hash(db, root));
    Ok(program)
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

/// The link set of the program's packages: each in program order under the
/// program's spelling of it, its edges resolved to set positions (an edge to
/// a package that emitted no output is dropped — nothing imports from it),
/// and its layout group from its root's kind.
fn link_set<'a>(db: &dyn baml_compiler2_hir::Db, packages: &'a [LinkedPackage]) -> LinkSet<'a> {
    let spelling = spelling(db);
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
                name: spelling.of(package.root).clone(),
                group: if package.root.kind(db) == SourceRootKind::Stdlib {
                    LinkGroup::Stdlib
                } else {
                    LinkGroup::User
                },
                edges: package
                    .edges
                    .iter()
                    .filter_map(|(edge, root)| Some((edge.clone(), position(*root)?)))
                    .collect(),
                unit: &package.emitted.unit,
                record: &package.emitted.record,
                tail: package.emitted.tail.as_ref(),
            })
            .collect(),
    }
}
