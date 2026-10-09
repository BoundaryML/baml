//! An ahead-of-time backend that turns BAML MIR into Rust source.
//!
//! The subset covers free functions over `int`, `bool`, `float`, `string`,
//! arrays, non-generic data classes and `T | null`, with structured control
//! flow, direct calls to other such functions, the for-in iterator protocol
//! on arrays, and a table of stdlib builtins mapped to the `bex_aot`
//! runtime. Everything else is reported as [`Rejection::Unsupported`] so the
//! caller keeps the bytecode. Control flow is emitted structurally (labeled
//! blocks and loops, never a block dispatcher), which needs a reducible
//! graph; BAML lowering only produces those, so an irreducible one is
//! [`Rejection::Invalid`].
//!
//! The generated code links the `bex_aot` runtime crate for value
//! semantics and panic payloads; this crate only names its paths.
//!
//! See `README.md` for the subset, the emission scheme and the rejections.

use baml_compiler2_hir::loc::FunctionLoc;
use rustc_hash::{FxHashMap, FxHashSet};

mod classes;
mod function;
mod print;
mod project;
mod structure;
mod types;

pub use project::{ProjectOptions, write_project};
pub use types::NativeTy;

/// Why a function was not compiled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// The function uses something outside the native subset. The bytecode
    /// stays authoritative for it.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The MIR violates an invariant a checked program's MIR must hold: a
    /// compiler bug, never a reason to fall back silently.
    #[error("invalid MIR: {0}")]
    Invalid(String),
}

impl Rejection {
    pub(crate) fn unsupported(reason: impl Into<String>) -> Self {
        Self::Unsupported(reason.into())
    }

    pub(crate) fn invalid(reason: impl Into<String>) -> Self {
        Self::Invalid(reason.into())
    }

    fn in_function(self, link_name: &str) -> Self {
        match self {
            Self::Unsupported(reason) => Self::Unsupported(format!("{link_name}: {reason}")),
            Self::Invalid(reason) => Self::Invalid(format!("{link_name}: {reason}")),
        }
    }
}

/// One function of a [`NativeModule`].
#[derive(Debug, Clone)]
pub struct CompiledFunction<'db> {
    pub loc: FunctionLoc<'db>,
    /// The BAML link name, e.g. `user.f`.
    pub link_name: String,
    /// The Rust function's identifier in the generated module.
    pub rust_name: String,
    /// BAML parameter names and native types, in declaration order.
    pub params: Vec<(String, NativeTy<'db>)>,
    /// The return type; [`NativeTy::Null`] for `void`.
    pub ret: NativeTy<'db>,
}

impl CompiledFunction<'_> {
    /// Whether the host shim can call this function from the command line:
    /// every parameter is an `int`, `bool`, `float` or `string`. Otherwise a
    /// project built around it is usable as a library only.
    pub fn shim_callable(&self) -> bool {
        self.params.iter().all(|(_, ty)| ty.is_shim_argument())
    }
}

/// What [`admit`] learned about an admitted function.
#[derive(Debug, Clone)]
pub struct Admitted<'db> {
    /// BAML parameter names and native types, in declaration order.
    pub params: Vec<(String, NativeTy<'db>)>,
    /// The return type; [`NativeTy::Null`] for `void`.
    pub ret: NativeTy<'db>,
}

impl Admitted<'_> {
    /// See [`CompiledFunction::shim_callable`].
    pub fn shim_callable(&self) -> bool {
        self.params.iter().all(|(_, ty)| ty.is_shim_argument())
    }
}

/// One class of a [`NativeModule`], emitted as a struct.
#[derive(Debug, Clone)]
pub struct CompiledClass {
    /// The BAML link name, e.g. `user.State`.
    pub link_name: String,
    /// The generated struct's identifier.
    pub rust_name: String,
}

/// The Rust module compiled from one or more root functions and their
/// callees.
#[derive(Debug, Clone)]
pub struct NativeModule<'db> {
    /// The generated `lib.rs`.
    pub rust_source: String,
    /// The MIR of every compiled function, as the pretty-printer shows it.
    pub mir_dump: String,
    /// Compiled functions, callees before their callers.
    pub functions: Vec<CompiledFunction<'db>>,
    /// Every class the functions touch, in first-use order.
    pub classes: Vec<CompiledClass>,
    /// Indices into `functions` of the requested roots, in request order.
    pub roots: Vec<usize>,
    /// Index of the entry function in `functions`: the root [`compile`] was
    /// given, or the first root [`compile_many`] was given. [`write_project`]
    /// builds the host shim around it.
    pub entry: usize,
}

/// Whether `loc` alone is in the subset. Callees are not inspected; the
/// classes its signature and body mention are. [`admit_closure`] answers
/// whether `loc` can actually be compiled.
pub fn admit<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    loc: FunctionLoc<'db>,
) -> Result<Admitted<'db>, Rejection> {
    let mut classes = classes::ClassTable::new(db);
    let candidate = function::analyze(db, loc, &mut classes)
        .map_err(|rejection| rejection.in_function(&link_name(db, loc)))?;
    Ok(candidate.admitted())
}

/// Whether `loc` and every function it transitively calls are in the
/// subset: what [`compile`] would compile, without printing. A rejection
/// names the callee it comes from.
pub fn admit_closure<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    loc: FunctionLoc<'db>,
) -> Result<Admitted<'db>, Rejection> {
    let mut classes = classes::ClassTable::new(db);
    let mut graph = CallGraph::new(db);
    graph.visit(loc, &mut classes)?;
    classes.finish()?;
    let candidate = graph
        .order
        .iter()
        .find(|candidate| candidate.loc == loc)
        .expect("a visited root is in the order");
    Ok(candidate.admitted())
}

/// Compile `entry` and every function it transitively calls, recording
/// `entry` as the module's entry point.
pub fn compile<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    entry: FunctionLoc<'db>,
) -> Result<NativeModule<'db>, Rejection> {
    compile_many(db, &[entry])
}

/// The Rust identifier the generated module gives the function `loc`: see
/// [`rust_name`].
pub fn rust_name_for(db: &dyn baml_compiler2_mir::Db, loc: FunctionLoc<'_>) -> String {
    rust_name(&link_name(db, loc))
}

/// The Rust identifier for a BAML link name: every character other than an
/// ASCII letter or digit becomes `_`, and a leading digit gets a `_` prefix.
/// `user.sum_of_squares` becomes `user_sum_of_squares`; the class `user.Cell`
/// becomes the struct `user_Cell`. Two names that sanitize alike cannot share
/// a module.
pub fn rust_name(link_name: &str) -> String {
    let mut name: String = link_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    name
}

/// A BAML-flavoured description of `ty`, with classes by link name.
pub fn describe_ty(db: &dyn baml_compiler2_mir::Db, ty: &NativeTy<'_>) -> String {
    ty.describe(&|class| baml_compiler2_mir::class_link_name(db, class))
}

/// Compile every function in `roots` and every function they transitively
/// call into one module. Functions reached more than once are compiled once;
/// callees precede their callers. The first root is the module's entry.
pub fn compile_many<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    roots: &[FunctionLoc<'db>],
) -> Result<NativeModule<'db>, Rejection> {
    let [first_root, ..] = roots else {
        return Err(Rejection::unsupported("no root function to compile"));
    };
    let mut classes = classes::ClassTable::new(db);
    let mut graph = CallGraph::new(db);
    for &root in roots {
        graph.visit(root, &mut classes)?;
    }
    let CallGraph {
        mut order,
        recursive,
        ..
    } = graph;
    for candidate in &mut order {
        candidate.recursive = recursive.contains(&candidate.loc);
    }

    // Every call site agrees with its callee's declared shape.
    let index_of: FxHashMap<FunctionLoc<'db>, usize> = order
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.loc, index))
        .collect();
    for candidate in &order {
        for (block, call) in &candidate.calls {
            let function::CallKind::Direct {
                callee,
                args,
                result,
                ..
            } = call
            else {
                continue;
            };
            let callee = &order[index_of[callee]];
            let mismatch = |what: &str| {
                Rejection::invalid(format!(
                    "{}: call of `{}` in {block} {what}",
                    candidate.link_name, callee.link_name
                ))
            };
            if args.len() != callee.arity() {
                return Err(mismatch("passes the wrong number of arguments"));
            }
            if args.iter().collect::<Vec<_>>() != callee.param_tys() {
                return Err(mismatch("passes arguments of the wrong type"));
            }
            if result != callee.return_ty() {
                return Err(mismatch("expects a result of the wrong type"));
            }
        }
    }

    let names = rust_names(&order)?;
    let class_infos = classes.finish()?;
    let rust_source = print::render_module(&class_infos, &order, &names, &classes)?;
    let mir_dump = order
        .iter()
        .map(|candidate| baml_compiler2_mir::pretty::display_function(db, candidate.mir))
        .collect::<Vec<_>>()
        .join("\n");
    let functions = order
        .iter()
        .map(|candidate| CompiledFunction {
            loc: candidate.loc,
            link_name: candidate.link_name.clone(),
            rust_name: names[&candidate.loc].to_string(),
            params: candidate
                .param_names
                .iter()
                .zip(candidate.param_tys())
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
            ret: candidate.return_ty().clone(),
        })
        .collect();
    let compiled_classes = class_infos
        .iter()
        .map(|info| CompiledClass {
            link_name: info.link_name.clone(),
            rust_name: info.ident.to_string(),
        })
        .collect();
    Ok(NativeModule {
        rust_source,
        mir_dump,
        functions,
        classes: compiled_classes,
        roots: roots.iter().map(|root| index_of[root]).collect(),
        entry: index_of[first_root],
    })
}

fn link_name(db: &dyn baml_compiler2_mir::Db, loc: FunctionLoc<'_>) -> String {
    baml_compiler2_mir::definition_link_name(
        db,
        baml_compiler2_hir::contributions::Definition::Function(loc),
    )
}

enum VisitState {
    Visiting,
    Done,
}

/// Depth-first traversal of the admitted call graph, callees first (as far
/// as cycles allow).
struct CallGraph<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    order: Vec<function::Candidate<'db>>,
    state: FxHashMap<FunctionLoc<'db>, VisitState>,
    /// The functions on the current path, outermost first.
    path: Vec<FunctionLoc<'db>>,
    /// Functions on a call cycle: each guards its recursion depth.
    recursive: FxHashSet<FunctionLoc<'db>>,
}

impl<'db> CallGraph<'db> {
    fn new(db: &'db dyn baml_compiler2_mir::Db) -> Self {
        Self {
            db,
            order: Vec::new(),
            state: FxHashMap::default(),
            path: Vec::new(),
            recursive: FxHashSet::default(),
        }
    }

    fn visit(
        &mut self,
        loc: FunctionLoc<'db>,
        classes: &mut classes::ClassTable<'db>,
    ) -> Result<(), Rejection> {
        match self.state.get(&loc) {
            Some(VisitState::Done) => return Ok(()),
            Some(VisitState::Visiting) => {
                // A call back into the path: every function from there on is
                // on the cycle. The VM bounds recursion by its frame count
                // and so does the generated code, through a depth guard.
                let start = self
                    .path
                    .iter()
                    .position(|on_path| *on_path == loc)
                    .expect("a function being visited is on the path");
                self.recursive.extend(self.path[start..].iter().copied());
                return Ok(());
            }
            None => {}
        }
        let name = link_name(self.db, loc);
        self.state.insert(loc, VisitState::Visiting);
        self.path.push(loc);
        let candidate = function::analyze(self.db, loc, classes)
            .map_err(|rejection| rejection.in_function(&name))?;
        let callees: Vec<FunctionLoc<'db>> = candidate
            .calls
            .iter()
            .filter_map(|(_, call)| match call {
                function::CallKind::Direct { callee, .. } => Some(*callee),
                function::CallKind::Panic(_) | function::CallKind::Builtin { .. } => None,
            })
            .collect();
        for callee in callees {
            self.visit(callee, classes)?;
        }
        self.path.pop();
        self.state.insert(loc, VisitState::Done);
        self.order.push(candidate);
        Ok(())
    }
}

/// Each candidate's [`rust_name`], as an identifier. A collision between two
/// link names is reported rather than renamed, so a caller can predict every
/// name from the link name alone.
fn rust_names<'db>(
    candidates: &[function::Candidate<'db>],
) -> Result<FxHashMap<FunctionLoc<'db>, proc_macro2::Ident>, Rejection> {
    let mut by_name: FxHashMap<String, &str> = FxHashMap::default();
    let mut names = FxHashMap::default();
    for candidate in candidates {
        let name = rust_name(&candidate.link_name);
        if let Some(other) = by_name.insert(name.clone(), &candidate.link_name) {
            return Err(Rejection::unsupported(format!(
                "`{}` and `{other}` both need the Rust name `{name}`",
                candidate.link_name
            )));
        }
        names.insert(
            candidate.loc,
            proc_macro2::Ident::new(&name, proc_macro2::Span::call_site()),
        );
    }
    Ok(names)
}
