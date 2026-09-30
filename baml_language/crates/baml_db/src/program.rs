//! A program from its packages: every package of the program emitted on its
//! own ([`emit_package`]) — or served by a [`PackageCache`] that still holds
//! its current output — then bound and laid out by the linker.
//!
//! This is the whole of what knows about a "program" — the emitter compiles
//! one package against its dependencies' interfaces and the linker binds
//! outputs it is handed; neither sees the other. A driver that emits
//! packages in parallel across the dependency DAG changes only this file.

use baml_base::{Name, SourceRoot};
use baml_compiler2_emit::{LoweringError, OptLevel, emit_package, project_source_content_hash};
use baml_compiler2_hir::package::{edge_table, spelling, world_roots};
use baml_linker::{LinkError, LinkPackage, LinkPackageId, LinkSet, link};
pub use baml_linker_types::EmittedPackage;
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
    /// A package served from its interface — one with no source to emit —
    /// has no compiled output in the store, so nothing can stand in the
    /// program for it.
    ServedWithoutOutput { package: Name },
}

impl std::fmt::Display for CompileProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
                            package: spelling(db).of(package).clone(),
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
                edges: edge_table(db, package),
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
/// ([`program_order`]) under the program's spelling of it, its edges
/// resolved to set positions (an edge to a package that emitted no output
/// is dropped — nothing imports from it).
fn link_set<'a>(
    db: &dyn baml_compiler2_hir::Db,
    packages: &'a [LinkedPackage],
    root: SourceRoot,
) -> LinkSet<'a> {
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
                edges: package
                    .edges
                    .iter()
                    .filter_map(|(edge, target)| {
                        Some(baml_linker::LinkEdge {
                            name: edge.clone(),
                            target: position(*target)?,
                            // The language packages are the prelude: reached
                            // under fixed names, never declared.
                            kind: if target.kind(db) == baml_base::SourceRootKind::Stdlib {
                                bex_vm_types::types::EdgeKind::Prelude
                            } else {
                                bex_vm_types::types::EdgeKind::Declared
                            },
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
