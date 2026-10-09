//! An ahead-of-time backend that turns BAML MIR into Rust source.
//!
//! The subset covers functions over `int`, `bool`, `float`, `bigint`,
//! `string`, arrays, maps, enums, data classes (generic ones as one struct
//! per instantiation), `T | null`, closed unions of those (generated enums,
//! with the narrowing the checker does), interface-typed values (generated
//! enums over the program's implementors, dispatched by `match`; a call on a
//! concrete receiver is static) and function values (lambdas with captures,
//! declared functions as values, calls through either), with structured
//! control flow, direct calls to other such functions (constant defaults
//! filled at the call site; a generic callee compiled once per
//! type-argument tuple), the for-in iterator protocol on arrays, and a
//! table of stdlib builtins mapped to the `bex_aot` runtime, the array
//! methods that take a callback among them. Everything else is reported as
//! [`Rejection::Unsupported`] so the caller keeps the bytecode. Control flow
//! is emitted structurally (labeled blocks and loops, never a block
//! dispatcher), which needs a reducible graph; BAML lowering only produces
//! those, so an irreducible one is [`Rejection::Invalid`].
//!
//! The generated code links the `bex_aot` runtime crate for value
//! semantics and panic payloads; this crate only names its paths.
//!
//! See `README.md` for the subset, the emission scheme and the rejections.

use baml_compiler2_hir::loc::FunctionLoc;
use rustc_hash::{FxHashMap, FxHashSet};

mod classes;
mod function;
mod generics;
mod print;
mod project;
mod structure;
mod types;
mod unions;

pub use function::FnId;
pub use generics::Instance;
pub use project::{ProjectOptions, write_project};
pub use types::{ClassInst, NativeTy, TypeDecl};

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

    /// `what of <reason>`, for a rejected type in a named position.
    pub(crate) fn in_what(self, what: &str) -> Self {
        match self {
            Self::Unsupported(reason) => Self::Unsupported(format!("{what} of {reason}")),
            invalid @ Self::Invalid(_) => invalid,
        }
    }
}

/// One function of a [`NativeModule`]: a declaration, or one instance of a
/// generic declaration.
#[derive(Debug, Clone)]
pub struct CompiledFunction<'db> {
    pub loc: FunctionLoc<'db>,
    /// The instance's type arguments, empty for a non-generic function.
    pub type_args: Vec<NativeTy<'db>>,
    /// The BAML link name, e.g. `user.f`, or `user.f<int>` for an instance.
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

/// One class of a [`NativeModule`], emitted as a struct, one enum, emitted
/// as a fieldless Rust enum, or one union, emitted as a Rust enum over its
/// members.
#[derive(Debug, Clone)]
pub struct CompiledClass {
    /// The BAML link name, e.g. `user.State`.
    pub link_name: String,
    /// The generated item's identifier.
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
    /// Every enum the functions touch, in first-use order.
    pub enums: Vec<CompiledClass>,
    /// Every closed union the functions touch, in first-use order, each a
    /// generated enum; `link_name` is the BAML spelling (`int | float`).
    pub unions: Vec<CompiledClass>,
    /// Every interface the functions touch, in first-use order, each a
    /// generated enum over its implementors.
    pub interfaces: Vec<CompiledClass>,
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
    let mut classes = classes::ClassTable::new(db, Some(program_root(db, loc)));
    let candidate = function::analyze(db, &Instance::plain(loc), &mut classes)
        .map_err(|rejection| rejection.in_function(&link_name(db, loc)))?;
    classes.finish()?;
    Ok(candidate.admitted())
}

/// The package `loc` is compiled from: what decides which `implements`
/// blocks the program can see.
fn program_root(db: &dyn baml_compiler2_mir::Db, loc: FunctionLoc<'_>) -> baml_base::SourceRoot {
    baml_compiler2_hir::file_package::file_package(
        db,
        baml_compiler2_hir::contributions::Definition::Function(loc).file(db),
    )
    .root
}

/// Whether `loc` and every function it transitively calls are in the
/// subset: what [`compile`] would compile, without printing. A rejection
/// names the callee it comes from.
pub fn admit_closure<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    loc: FunctionLoc<'db>,
) -> Result<Admitted<'db>, Rejection> {
    let mut classes = classes::ClassTable::new(db, Some(program_root(db, loc)));
    let mut graph = CallGraph::new(db);
    let root = Instance::plain(loc);
    graph.visit(&root, &mut classes)?;
    classes.finish()?;
    let candidate = graph
        .order
        .iter()
        .find(|candidate| candidate.id == FnId::Declared(root.clone()))
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

/// A BAML-flavoured description of `ty`, with classes and enums by link
/// name.
pub fn describe_ty(db: &dyn baml_compiler2_mir::Db, ty: &NativeTy<'_>) -> String {
    let classes = classes::ClassTable::new(db, None);
    ty.describe(&|decl| classes.link_name(decl))
}

/// Compile every function in `roots` and every function they transitively
/// call into one module. Functions reached more than once are compiled once;
/// callees precede their callers, and a lambda precedes the function that
/// creates it. The first root is the module's entry.
pub fn compile_many<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    roots: &[FunctionLoc<'db>],
) -> Result<NativeModule<'db>, Rejection> {
    let [first_root, ..] = roots else {
        return Err(Rejection::unsupported("no root function to compile"));
    };
    let mut classes = classes::ClassTable::new(db, Some(program_root(db, *first_root)));
    let mut graph = CallGraph::new(db);
    for &root in roots {
        graph.visit(&Instance::plain(root), &mut classes)?;
    }
    let CallGraph {
        mut order,
        recursive,
        ..
    } = graph;
    // A function on a static call cycle guards its depth. So does every
    // lambda and every function that calls through a function value: a
    // cycle closed through a function value is not in the static graph, and
    // every such cycle passes through one of those.
    for candidate in &mut order {
        candidate.recursive =
            recursive.contains(&candidate.id) || candidate.is_lambda() || candidate.indirect;
    }

    // Every call site agrees with its callee's declared shape.
    let index_of: FxHashMap<FnId<'db>, usize> = order
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.id.clone(), index))
        .collect();
    for candidate in &order {
        for (block, call) in &candidate.calls {
            let targets: Vec<&function::DirectCall<'db>> = match call {
                function::CallKind::Direct(target) => vec![target],
                function::CallKind::Virtual { arms, .. } => arms.iter().collect(),
                function::CallKind::Builtin {
                    builtin: function::Builtin::CompareVia { cmp, .. },
                    ..
                } => vec![cmp],
                function::CallKind::Panic(_)
                | function::CallKind::Builtin { .. }
                | function::CallKind::Indirect { .. } => continue,
            };
            for function::DirectCall {
                callee,
                args,
                result,
                ..
            } in targets
            {
                let callee = &order[index_of[&FnId::Declared(callee.clone())]];
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
    }

    let names = rust_names(db, &order, &classes)?;
    let (class_infos, enum_infos, union_infos, iface_infos) = classes.finish()?;
    let rust_source = print::render_module(
        &class_infos,
        &enum_infos,
        &union_infos,
        &iface_infos,
        &order,
        &names,
        &classes,
    )?;
    // A lambda's MIR is printed with the function that creates it, and a
    // generic function's once, whatever its instances.
    let mut dumped: FxHashSet<FunctionLoc<'db>> = FxHashSet::default();
    let mir_dump = order
        .iter()
        .filter(|candidate| !candidate.is_lambda())
        .filter(|candidate| dumped.insert(candidate.id.root().loc))
        .map(|candidate| baml_compiler2_mir::pretty::display_function(db, candidate.mir))
        .collect::<Vec<_>>()
        .join("\n");
    // Lambdas are emitted but have no declaration to list.
    let functions = order
        .iter()
        .filter_map(|candidate| match &candidate.id {
            FnId::Declared(instance) => Some((instance, candidate)),
            FnId::Lambda { .. } => None,
        })
        .map(|(instance, candidate)| CompiledFunction {
            loc: instance.loc,
            type_args: candidate.type_args.clone(),
            link_name: candidate.link_name.clone(),
            rust_name: names[&candidate.id].to_string(),
            params: candidate
                .param_names
                .iter()
                .zip(candidate.param_tys())
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
            ret: candidate.return_ty().clone(),
        })
        .collect::<Vec<CompiledFunction<'db>>>();
    let function_index: FxHashMap<FnId<'db>, usize> = order
        .iter()
        .filter(|candidate| !candidate.is_lambda())
        .enumerate()
        .map(|(index, candidate)| (candidate.id.clone(), index))
        .collect();
    let compiled_classes = class_infos
        .iter()
        .map(|info| CompiledClass {
            link_name: info.link_name.clone(),
            rust_name: info.ident.to_string(),
        })
        .collect();
    let compiled_enums = enum_infos
        .iter()
        .map(|info| CompiledClass {
            link_name: info.link_name.clone(),
            rust_name: info.ident.to_string(),
        })
        .collect();
    let compiled_unions = union_infos
        .iter()
        .map(|info| CompiledClass {
            link_name: info.described.clone(),
            rust_name: info.ident.to_string(),
        })
        .collect();
    let compiled_interfaces = iface_infos
        .iter()
        .map(|info| CompiledClass {
            link_name: info.link_name.clone(),
            rust_name: info.union.ident.to_string(),
        })
        .collect();
    Ok(NativeModule {
        rust_source,
        mir_dump,
        functions,
        classes: compiled_classes,
        enums: compiled_enums,
        unions: compiled_unions,
        interfaces: compiled_interfaces,
        roots: roots
            .iter()
            .map(|root| function_index[&FnId::Declared(Instance::plain(*root))])
            .collect(),
        entry: function_index[&FnId::Declared(Instance::plain(*first_root))],
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

/// How many instances of one generic function may be on the call path at
/// once: a function that calls itself at a growing type argument
/// (`f<T>` calling `f<T[]>`) has no finite set of instances, and is
/// rejected once the path holds this many.
const INSTANCE_DEPTH_LIMIT: usize = 16;

/// Depth-first traversal of the admitted call graph, callees first (as far
/// as cycles allow), over instances: a generic function is visited once per
/// type-argument tuple it is called at. A declared function used as a value
/// is a callee too; a lambda is visited with the function that creates it,
/// and its calls are the creator's.
struct CallGraph<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    order: Vec<function::Candidate<'db>>,
    state: FxHashMap<Instance<'db>, VisitState>,
    /// The instances on the current path, outermost first.
    path: Vec<Instance<'db>>,
    /// Functions on a call cycle: each guards its recursion depth.
    recursive: FxHashSet<FnId<'db>>,
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
        instance: &Instance<'db>,
        classes: &mut classes::ClassTable<'db>,
    ) -> Result<(), Rejection> {
        match self.state.get(instance) {
            Some(VisitState::Done) => return Ok(()),
            Some(VisitState::Visiting) => {
                // A call back into the path: every function from there on is
                // on the cycle. The VM bounds recursion by its frame count
                // and so does the generated code, through a depth guard.
                let start = self
                    .path
                    .iter()
                    .position(|on_path| on_path == instance)
                    .expect("a function being visited is on the path");
                self.recursive.extend(
                    self.path[start..]
                        .iter()
                        .map(|instance| FnId::Declared(instance.clone())),
                );
                return Ok(());
            }
            None => {}
        }
        let name = link_name(self.db, instance.loc);
        if instance.is_generic()
            && self
                .path
                .iter()
                .filter(|on_path| on_path.loc == instance.loc)
                .count()
                >= INSTANCE_DEPTH_LIMIT
        {
            return Err(Rejection::unsupported(format!(
                "{name}: instantiated at {INSTANCE_DEPTH_LIMIT} type arguments along one call path (unbounded polymorphic recursion)"
            )));
        }
        self.state.insert(instance.clone(), VisitState::Visiting);
        self.path.push(instance.clone());
        let candidate = function::analyze(self.db, instance, classes)
            .map_err(|rejection| rejection.in_function(&name))?;
        let tree = candidate.flatten();
        let mut callees: Vec<Instance<'db>> = Vec::new();
        for member in &tree {
            for (_, call) in &member.calls {
                match call {
                    function::CallKind::Direct(target) => callees.push(target.callee.clone()),
                    function::CallKind::Virtual { arms, .. } => {
                        callees.extend(arms.iter().map(|arm| arm.callee.clone()));
                    }
                    function::CallKind::Builtin {
                        builtin: function::Builtin::CompareVia { cmp, .. },
                        ..
                    } => callees.push(cmp.callee.clone()),
                    function::CallKind::Panic(_)
                    | function::CallKind::Builtin { .. }
                    | function::CallKind::Indirect { .. } => {}
                }
            }
            callees.extend(member.function_values.keys().cloned());
        }
        for callee in callees {
            self.visit(&callee, classes)?;
        }
        self.path.pop();
        self.state.insert(instance.clone(), VisitState::Done);
        self.order.extend(tree);
        Ok(())
    }
}

/// Each candidate's [`rust_name`], as an identifier: an instance of a
/// generic function carries its type arguments mangled
/// (`user_identity__int`, `user_first__user_Box__int`), and a lambda is
/// named after the instance it is lowered inside and its position
/// (`user_f__lambda0`, nested `user_f__lambda0__lambda1`). A collision
/// between two names is reported rather than renamed, so a caller can
/// predict every name from the link name and the type arguments alone.
fn rust_names<'db>(
    db: &dyn baml_compiler2_mir::Db,
    candidates: &[function::Candidate<'db>],
    classes: &classes::ClassTable<'db>,
) -> Result<FxHashMap<FnId<'db>, proc_macro2::Ident>, Rejection> {
    let mut by_name: FxHashMap<String, String> = FxHashMap::default();
    let mut names = FxHashMap::default();
    let instance_names: FxHashMap<&Instance<'db>, String> = candidates
        .iter()
        .filter_map(|candidate| match &candidate.id {
            FnId::Declared(instance) => {
                Some((instance, instance_rust_name(db, candidate, classes)))
            }
            FnId::Lambda { .. } => None,
        })
        .collect();
    for candidate in candidates {
        let name = match &candidate.id {
            FnId::Declared(instance) => instance_names[instance].clone(),
            FnId::Lambda { root, path } => {
                let mut name = instance_names
                    .get(root)
                    .cloned()
                    .ok_or_else(|| Rejection::invalid("lambda without its creator"))?;
                for index in path {
                    name.push_str("__lambda");
                    name.push_str(&index.to_string());
                }
                name
            }
        };
        if let Some(other) = by_name.insert(name.clone(), candidate.link_name.clone()) {
            return Err(Rejection::unsupported(format!(
                "`{}` and `{other}` both need the Rust name `{name}`",
                candidate.link_name
            )));
        }
        names.insert(
            candidate.id.clone(),
            proc_macro2::Ident::new(&name, proc_macro2::Span::call_site()),
        );
    }
    Ok(names)
}

/// The Rust name of a declared candidate: its link name sanitized, then
/// `__` and each type argument mangled.
fn instance_rust_name<'db>(
    db: &dyn baml_compiler2_mir::Db,
    candidate: &function::Candidate<'db>,
    classes: &classes::ClassTable<'db>,
) -> String {
    let mut name = rust_name(&link_name(db, candidate.id.root().loc));
    for arg in &candidate.type_args {
        name.push_str("__");
        name.push_str(&arg.mangle(&|decl| classes.ident(decl)));
    }
    name
}
