//! A program from its packages: every package of the program emitted on its
//! own ([`emit_package`]), then bound and laid out by the linker.
//!
//! This is the whole of what knows about a "program" — the emitter compiles
//! one package against its dependencies' interfaces and the linker binds
//! units it is handed; neither sees the other. A driver that caches units
//! per package, or emits packages in parallel across the dependency DAG,
//! changes only this file.

use baml_base::{SourceRoot, SourceRootKind};
use baml_compiler2_emit::{EmittedPackage, LoweringError, OptLevel, emit_package};
use baml_compiler2_hir::package::{spelling, world_roots};
use baml_linker::{LinkError, LinkGroup, LinkPackage, LinkPackageId, LinkSet, link};
use bex_vm_types::Program;

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
/// closure, each package emitted on its own and the units linked.
///
/// # Errors
///
/// [`CompileProgramError`]: a package that fails to emit, or units that fail
/// to bind.
pub fn compile_program(
    db: &dyn baml_compiler2_emit::Db,
    root: SourceRoot,
    opt: OptLevel,
) -> Result<Program, CompileProgramError> {
    // A root without files declares nothing: no unit, and nothing to import
    // from it.
    let packages: Vec<EmittedPackage> = world_roots(db, root)
        .iter()
        .copied()
        .filter(|root| !root.files(db).is_empty())
        .map(|root| emit_package(db, root, opt))
        .collect::<Result<_, _>>()?;
    Ok(link(&link_set(db, &packages))?)
}

/// The link set of emitted packages: each in program order under the
/// program's spelling of it, its edges resolved to set positions (an edge to
/// a package that emitted no unit is dropped — nothing imports from it), and
/// its layout group from its root's kind.
fn link_set<'a>(db: &dyn baml_compiler2_hir::Db, packages: &'a [EmittedPackage]) -> LinkSet<'a> {
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
                unit: &package.unit,
                record: &package.record,
                tail: package.tail.as_ref(),
            })
            .collect(),
    }
}
