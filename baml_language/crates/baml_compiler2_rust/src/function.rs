//! Admission and analysis of one MIR function.
//!
//! [`analyze`] decides whether a function lies in the native subset and, if
//! so, records everything the printer needs: the native type of every local,
//! the calls it makes, and its structured control flow. Anything outside the
//! subset is [`Rejection::Unsupported`]; anything a checked program's MIR
//! must never contain is [`Rejection::Invalid`].
//!
//! Locals whose declared type has no native representation (`unknown`, an
//! iterator interface, a function type lowering sometimes gives a receiver
//! temp) are *refined*: they take the native type of their defining rvalues
//! when those agree, which is how the for-in protocol's `iter`/`next` temps
//! become `array::Iter<T>` and `Option<T>`.
//!
//! A temporary that only copies a handle so a later statement can read
//! through it (`_8 = copy _1; _7 = _8[_9]`) is an *alias* of its source: the
//! printer reads `_1` directly and never declares `_8`, so an array element
//! read costs no reference-count traffic ([`find_aliases`]).
//!
//! A `catch` or `defer` handler block *lands* with the thrown value in its
//! error local, which is typed [`NativeTy::Thrown`]; the companion
//! `baml.errors.Context` local is never materialized ([`LocalKind::Context`]).
//! A class arm narrows the error into a temporary of the class
//! (`narrow_bind`), which is typed as the class and remembers which error
//! local it narrows ([`Candidate::narrow_sources`]); a multi-arm `catch`
//! switches on the error's class tag ([`LocalKind::Tag`]).
//!
//! A union value is a generated enum. Every type test on one (`is_type`,
//! `is_type_tag`, a `narrow_bind`, an arm of a `switch` on its `type_tag`)
//! is resolved here to the variants it selects ([`Candidate::member_tests`]),
//! and a read of a union as one of its members, which lowering emits where
//! the checker narrowed the value, is a [`Coercion::Narrow`] the printer
//! turns into a `match` whose other arms are unreachable.

use baml_base::LangPackage;
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    item_data::{MethodOwner, function_data, method_owner},
    loc::{DeclRef, FunctionLoc},
    package::lang_roots,
};
use baml_compiler2_hir_ty::layout;
use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, Constant, IndexKind, IntrinsicOp, Local, MirFunction,
    MirFunctionBody, MirFunctionKind, Operand, OptLevel, Place, RealizedTy, RuntimeTy, Rvalue,
    ShortCircuitKind, Statement, StatementKind, SwitchKey, Terminator, TyTemplate, TypeTest,
    UnaryOp, function_link_name, lower_function,
};
use baml_type::{Int63, Literal};
use rustc_hash::FxHashMap;

use crate::{
    Rejection,
    classes::ClassTable,
    structure::{Cfg, Flow, Stmt, structurize},
    types::{Coercion, NativeTy, TypeDecl, coercion},
    unions::{MemberTest, TestSite, member_tag, tag_variants},
};

/// The link name of the stdlib function whose call is emitted as a panic.
const PANIC_LINK_NAME: &str = "baml.sys.panic";

/// The marker an unresolved local's read reports during refinement.
const UNRESOLVED: &str = "unresolved local";

/// The namespaces of the classes the native runtime raises as types of its
/// own (`bex_lang::Panic`, `bex_aot::errors`, `bex_aot::json`), so a
/// `catch` binding of one cannot recover a generated instance.
const RUNTIME_ERROR_NAMESPACES: &[&str] = &["baml.panics.", "baml.errors.", "baml.json."];

/// The native representation of a MIR local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalKind<'db> {
    /// A value of this type.
    Value(NativeTy<'db>),
    /// `reflect.Type`: a type value assigned by `load_type` and read only as
    /// the type argument of a generic call. Not declared in the output.
    Type,
    /// `never`: only ever the destination of a `baml.sys.panic` call, which
    /// returns before anything is assigned. Not declared in the output.
    Never,
    /// The `baml.errors.Context` a handler lands with beside the error: its
    /// stack trace and cause chain, which native code does not keep. Read
    /// only by `rethrow` and `throw_if_panic`, which carry the error alone;
    /// any other read is outside the subset. Not declared in the output.
    Context,
    /// The type tag of a value (`type_tag`), switched on by a multi-arm
    /// `match` on types or a multi-arm `catch`.
    Tag(TagSource),
}

/// Whose type tag a [`LocalKind::Tag`] local holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagSource {
    /// A caught error's class tag: the class's fully qualified name,
    /// declared as `&'static str`.
    Thrown,
    /// The tag of this union (or nullable union) local: the switch on it
    /// is a `match` on the local's variants, and the tag local is never
    /// declared.
    Union(Local),
}

impl<'db> LocalKind<'db> {
    pub(crate) fn value(&self) -> Option<&NativeTy<'db>> {
        match self {
            Self::Value(ty) => Some(ty),
            Self::Type | Self::Never | Self::Context | Self::Tag(_) => None,
        }
    }
}

/// Which `array::sort_*` a `Sortable.sort` on a primitive array becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortKind {
    Int,
    Float,
    Str,
}

/// Which `bex_aot::map` function a `baml.Map.*` call becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapOp {
    /// `m.length()` called as a method (the subscript-free `len` rvalue is
    /// the usual lowering).
    Length,
    Has,
    Get,
    Index,
    Set,
    Delete,
    Keys,
    Values,
    GetOrInsert,
    Clear,
}

/// Which `bex_aot::bigint` function a `baml.Bigint.*` method becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BigintOp {
    Abs,
    Pow,
    Isqrt,
    Ilog,
    ToInt,
    Parse,
}

/// A stdlib function mapped to a `bex_aot` call rather than compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Builtin<'db> {
    /// `baml.Array.push<T>(ty, arr, v)`; the result is the new length.
    ArrayPush,
    /// `baml.json.deserialize<T>(ty, s)`, monomorphized on `T`.
    JsonDeserialize(NativeTy<'db>),
    /// `baml.json.to_string(v)`.
    JsonToString,
    /// `baml._to_string_default<T>(ty, v)`, or `virtual_call to_string as
    /// baml.ToString` on a value with no `to_string` of its own.
    ToStringDefault,
    /// `baml.String.length` / `char_count`.
    StringLength,
    /// `baml.String.is_ascii`.
    StringIsAscii,
    /// `baml.Float.floor`.
    FloatFloor,
    /// `baml.Float.itrunc`.
    FloatItrunc,
    /// `baml.ops.equals_equals(x, null)`: `operand` indexes the value
    /// argument that is the `Option`.
    IsNull { operand: usize },
    /// `baml.ops.equals_equals(a, b)` on two values of one enum: the
    /// variants are the same.
    EnumEq,
    /// `baml.ops.equals_equals(a, b)` on two values that both fit this
    /// type, which implements `bex_aot::BamlEq`: `bex_aot::eq::equals`.
    Equals(NativeTy<'db>),
    /// `virtual_call iter as baml.iter.Iterable` on an array.
    Iter,
    /// `virtual_call next as baml.iter.Iterator` on an array iterator.
    Next,
    /// `virtual_call sort as baml.Sortable` on a primitive array.
    Sort(SortKind),
    /// `baml.Map.<op>(m, ..)`.
    Map(MapOp),
    /// `baml.Bigint.<op>(..)`.
    Bigint(BigintOp),
}

/// What a `Call` or `VirtualCall` terminator does in the generated code.
#[derive(Debug, Clone)]
pub(crate) enum CallKind<'db> {
    /// A direct call of another source function, admitted separately.
    Direct {
        callee: FunctionLoc<'db>,
        /// The callee's parameter types; each argument coerces to its own.
        args: Vec<NativeTy<'db>>,
        /// The callee's return type; the destination is it or its `| null`.
        result: NativeTy<'db>,
        /// Per argument, the callee's constant default when the call omits
        /// that argument: the printer passes the constant instead of the
        /// `<omitted>` sentinel. See [`default_constants`].
        substituted: Vec<Option<Constant<'db>>>,
    },
    /// `baml.sys.panic("...")`: returns the panic instead of calling.
    Panic(String),
    /// A `bex_aot` call; `result` is the destination's type.
    Builtin {
        builtin: Builtin<'db>,
        result: NativeTy<'db>,
    },
}

/// A function admitted to the subset, with everything the printer reads.
pub(crate) struct Candidate<'db> {
    pub loc: FunctionLoc<'db>,
    pub link_name: String,
    pub mir: &'db MirFunction<'db>,
    pub body: &'db MirFunctionBody<'db>,
    /// Parallel to `body.locals`.
    pub kinds: Vec<LocalKind<'db>>,
    /// BAML parameter names, parallel to `_1..=arity`.
    pub param_names: Vec<String>,
    /// What each `Call`/`VirtualCall` terminator does, by block.
    pub calls: Vec<(BlockId, CallKind<'db>)>,
    /// How each statement's value is stored into its destination, parallel
    /// to `body.blocks[..].statements`; `None` for a statement that stores
    /// nothing.
    pub stores: Vec<Vec<Option<Coercion>>>,
    /// The native type of each statement's value before the store, parallel
    /// to `stores`: what an array or map literal builds, which its
    /// destination (a union, say) does not spell.
    pub values: Vec<Vec<Option<NativeTy<'db>>>>,
    /// Temporaries the printer reads through their source place instead of
    /// declaring and assigning; see [`find_aliases`].
    pub aliases: FxHashMap<Local, Place>,
    pub structured: Vec<Stmt>,
    /// Per block, the jump to its unwind handler (`break` to the handler's
    /// label), for every throw or panic the block's code can raise; empty
    /// for a block with no handler. See [`crate::structure`].
    pub unwind_jumps: Vec<Vec<Stmt>>,
    /// For each `narrow_bind` temporary (typed as the class it narrows to),
    /// the handler error local it narrows: the printer downcasts that local
    /// and never emits the `copy` that seeded the temporary.
    pub narrow_sources: FxHashMap<Local, Local>,
    /// What every type test on a union or nullable value decides, by site.
    pub member_tests: FxHashMap<TestSite, MemberTest>,
    /// Whether the function is on a call cycle, so its body runs under a
    /// depth guard. Set by the call graph, not by [`analyze`].
    pub recursive: bool,
}

impl<'db> Candidate<'db> {
    pub(crate) fn arity(&self) -> usize {
        self.mir.arity
    }

    /// Parameter types, parallel to `_1..=arity`.
    pub(crate) fn param_tys(&self) -> Vec<&NativeTy<'db>> {
        self.kinds[1..=self.mir.arity]
            .iter()
            .map(|kind| kind.value().expect("parameters are values"))
            .collect()
    }

    pub(crate) fn return_ty(&self) -> &NativeTy<'db> {
        self.kinds[0].value().expect("the return place is a value")
    }

    /// The signature, as [`crate::admit`] reports it.
    pub(crate) fn admitted(&self) -> crate::Admitted<'db> {
        crate::Admitted {
            params: self
                .param_names
                .iter()
                .zip(self.param_tys())
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
            ret: self.return_ty().clone(),
        }
    }

    pub(crate) fn call(&self, block: BlockId) -> Option<&CallKind<'db>> {
        self.calls
            .iter()
            .find(|(id, _)| *id == block)
            .map(|(_, call)| call)
    }
}

/// Decide whether `loc` is in the subset and gather what emitting it needs.
/// Classes the function touches are registered in `classes`.
pub(crate) fn analyze<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    loc: FunctionLoc<'db>,
    classes: &mut ClassTable<'db>,
) -> Result<Candidate<'db>, Rejection> {
    let data = function_data(db, loc);
    if !data.generic_params.is_empty() {
        return Err(Rejection::unsupported("generic function"));
    }
    // A method of a concrete class, declared in the class or in an
    // `implements` block, is a function whose first parameter is the
    // receiver: calls to it are direct. Only an interface's default method,
    // whose receiver is `Self`, needs dispatch the subset lacks.
    if let Some(MethodOwner::Interface(_)) = method_owner(db, loc) {
        return Err(Rejection::unsupported("interface method"));
    }
    if baml_compiler2_hir_ty::infer::trace_hooks::declaration_plan(db, loc).is_some() {
        return Err(Rejection::unsupported("declared trace hook"));
    }
    let mir = lower_function(db, loc, OptLevel::One)
        .as_ref()
        .map_err(|error| Rejection::invalid(format!("MIR lowering failed: {error}")))?;
    let link_name = mir.identity.link_name(db);
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return Err(Rejection::unsupported("builtin function"));
    };
    // The constructs that define a whole feature area are reported before
    // anything they drag along (a `spawn` body is also a lambda), so the
    // admission report names the feature.
    for block in &body.blocks {
        match &block.terminator {
            Some(Terminator::Spawn { .. }) => return Err(Rejection::unsupported("spawn")),
            Some(Terminator::Await { .. } | Terminator::AwaitAny { .. }) => {
                return Err(Rejection::unsupported("await"));
            }
            Some(Terminator::SysOp { .. }) => return Err(Rejection::unsupported("sys-op call")),
            _ => {}
        }
    }
    if !mir.lambdas.is_empty() {
        return Err(Rejection::unsupported("lambda"));
    }
    if body.locals.len() <= mir.arity {
        return Err(Rejection::invalid("fewer locals than parameters"));
    }
    if body.blocks.is_empty() {
        return Err(Rejection::invalid("no basic blocks"));
    }

    for (index, block) in body.blocks.iter().enumerate() {
        if block.id.0 != index {
            return Err(Rejection::invalid(format!(
                "{} is stored at position {index}",
                block.id
            )));
        }
        // `shielded` marks a `defer` body, which the VM runs shielded from
        // cancellation; native code has no cancellation, so the mark is
        // nothing to act on.
        if block.terminator.is_none() {
            return Err(Rejection::invalid(format!(
                "{} has no terminator",
                block.id
            )));
        }
    }
    let param_names = data
        .params
        .iter()
        .map(|param| param.name.to_string())
        .collect::<Vec<_>>();
    if param_names.len() != mir.arity {
        return Err(Rejection::invalid(
            "parameter list disagrees with MIR arity",
        ));
    }
    let defaulted: Vec<bool> = data.params.iter().map(|param| param.has_default).collect();
    default_constants(body, &param_names, &defaulted)?;

    let mut env = Env {
        db,
        body,
        kinds: Vec::with_capacity(body.locals.len()),
        type_values: FxHashMap::default(),
        classes,
        narrows: FxHashMap::default(),
        narrow_sources: FxHashMap::default(),
        member_tests: FxHashMap::default(),
    };
    let handler_kinds = env.handler_locals()?;
    let unresolved = env.declare_locals(mir.arity, &handler_kinds)?;
    env.register_enum_constants()?;
    env.refine(unresolved)?;
    env.union_tags()?;
    let kinds: Vec<LocalKind<'db>> = env
        .kinds
        .iter()
        .map(|kind| {
            kind.clone()
                .expect("every local is resolved after refinement")
        })
        .collect();

    let mut flows = Vec::with_capacity(body.blocks.len());
    let mut calls = Vec::new();
    let mut stores = Vec::with_capacity(body.blocks.len());
    let mut values = Vec::with_capacity(body.blocks.len());
    for block in &body.blocks {
        let mut block_stores = Vec::with_capacity(block.statements.len());
        let mut block_values = Vec::with_capacity(block.statements.len());
        for (index, statement) in block.statements.iter().enumerate() {
            let stored = env.statement(block.id, index, &statement.kind)?;
            block_stores.push(stored.as_ref().map(|(store, _)| *store));
            block_values.push(stored.map(|(_, actual)| actual));
        }
        stores.push(block_stores);
        values.push(block_values);
        let terminator = block.terminator.as_ref().expect("checked above");
        let (flow, call) = env.terminator(block.id, terminator)?;
        flows.push(flow);
        if let Some(call) = call {
            calls.push((block.id, call));
        }
    }

    // Reads of an unassigned local are rustc's to reject: every local is
    // declared without an initializer and the structured output has the
    // same paths as the MIR, so rustc's definite-initialization check runs
    // over the same graph a backend pass would.
    let cfg = Cfg {
        entry: body.entry.0,
        flows,
        unwinds: body
            .blocks
            .iter()
            .map(|block| block.unwind.map(|handler| handler.0))
            .collect(),
    };
    let structured = structurize(&cfg).map_err(|error| Rejection::invalid(error.to_string()))?;
    let aliases = find_aliases(body, mir.arity, &kinds, &stores);
    let narrow_sources = env.narrow_sources;
    let member_tests = env.member_tests;

    Ok(Candidate {
        loc,
        link_name,
        mir,
        body,
        kinds,
        param_names,
        calls,
        stores,
        values,
        aliases,
        structured: structured.stmts,
        unwind_jumps: structured.unwind_jumps,
        narrow_sources,
        member_tests,
        recursive: false,
    })
}

/// A definition of a local, for refinement.
enum Def<'a, 'db> {
    Rvalue(&'a Rvalue<'db>),
    Terminator(&'a Terminator<'db>),
}

/// Types the locals, statements and terminators of one function.
struct Env<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    body: &'db MirFunctionBody<'db>,
    /// Parallel to `body.locals`; `None` while a local awaits refinement.
    kinds: Vec<Option<LocalKind<'db>>>,
    /// The template each `reflect.Type` local is loaded with.
    type_values: FxHashMap<Local, TyTemplate>,
    classes: &'a mut ClassTable<'db>,
    /// The class each `narrow_bind` temporary narrows to.
    narrows: FxHashMap<Local, baml_compiler2_hir_ty::extern_loc::ClassRef<'db>>,
    /// See [`Candidate::narrow_sources`].
    narrow_sources: FxHashMap<Local, Local>,
    /// See [`Candidate::member_tests`].
    member_tests: FxHashMap<TestSite, MemberTest>,
}

impl<'db> Env<'_, 'db> {
    /// The kinds of the locals `catch` and `defer` handlers use, which their
    /// declarations (`unknown`, or `int` for a tag) do not say: a handler's
    /// error local holds the thrown value and its context local nothing
    /// native, a `narrow_bind` temporary is the class it narrows the error
    /// to, and the `type_tag` of a thrown value is its class tag.
    fn handler_locals(&mut self) -> Result<FxHashMap<Local, LocalKind<'db>>, Rejection> {
        let mut kinds = FxHashMap::default();
        for (_, landing) in self.body.handlers() {
            kinds.insert(landing.error_local, LocalKind::Value(NativeTy::Thrown));
            kinds.insert(landing.context_local, LocalKind::Context);
        }
        let is_thrown = |kinds: &FxHashMap<Local, LocalKind<'db>>, local: &Local| {
            kinds.get(local) == Some(&LocalKind::Value(NativeTy::Thrown))
        };
        // A `narrow_bind` temporary seeded with a caught error (`_t = copy
        // _e` in the handler) is a `catch` arm binding; any other
        // `narrow_bind` is a narrowing pattern on a union or an interface,
        // which the subset does not have.
        let mut seeded: Vec<Local> = Vec::new();
        for block in &self.body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(destination),
                    value:
                        Rvalue::Use(
                            Operand::Copy(Place::Local(source))
                            | Operand::Move(Place::Local(source)),
                        ),
                } = &statement.kind
                    && is_thrown(&kinds, source)
                {
                    seeded.push(*destination);
                }
            }
        }
        for block in &self.body.blocks {
            let Some(Terminator::NarrowBind {
                test, destination, ..
            }) = &block.terminator
            else {
                continue;
            };
            if !seeded.contains(destination) {
                continue;
            }
            let class = match test {
                TypeTest::Class { class, args } if args.is_empty() => *class,
                TypeTest::Class { .. } => {
                    return Err(Rejection::unsupported("`catch` binding of a generic class"));
                }
                TypeTest::Enum(_) | TypeTest::Template(_) => {
                    return Err(Rejection::unsupported(
                        "`catch` binding of a type other than a class",
                    ));
                }
            };
            // The runtime raises the stdlib's own errors and panics as its
            // own types, which the generated struct of the class would never
            // downcast to; the class test still matches.
            let link_name = baml_compiler2_mir::class_link_name(self.db, class);
            if RUNTIME_ERROR_NAMESPACES
                .iter()
                .any(|namespace| link_name.starts_with(namespace))
            {
                return Err(Rejection::unsupported(format!(
                    "`catch` binding of the stdlib class `{link_name}` (only its class test is native)"
                )));
            }
            self.classes
                .check(class)
                .map_err(|reason| Rejection::unsupported(reason.0))?;
            if let Some(previous) = self.narrows.insert(*destination, class)
                && previous != class
            {
                return Err(Rejection::invalid(format!(
                    "{destination} is narrowed to two classes"
                )));
            }
            kinds.insert(*destination, LocalKind::Value(NativeTy::Class(class)));
        }
        for block in &self.body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(destination),
                    value: Rvalue::TypeTag(Place::Local(source)),
                } = &statement.kind
                    && is_thrown(&kinds, source)
                {
                    kinds.insert(*destination, LocalKind::Tag(TagSource::Thrown));
                }
            }
        }
        Ok(kinds)
    }

    /// Type every local from its declaration. Locals whose declared type has
    /// no native representation are returned (with the reason) for
    /// refinement; a parameter or the return place must be representable.
    fn declare_locals(
        &mut self,
        arity: usize,
        handler_kinds: &FxHashMap<Local, LocalKind<'db>>,
    ) -> Result<Vec<(Local, Rejection)>, Rejection> {
        let mut unresolved = Vec::new();
        for (index, local) in self.body.locals.iter().enumerate() {
            if local.is_captured {
                return Err(Rejection::unsupported("captured local"));
            }
            let is_param = (1..=arity).contains(&index);
            let is_signature = is_param || index == 0;
            let kind = match &local.ty {
                _ if handler_kinds.contains_key(&Local(index)) && !is_signature => {
                    Some(handler_kinds[&Local(index)].clone())
                }
                RuntimeTy::Type if !is_signature => Some(LocalKind::Type),
                RuntimeTy::Never if !is_signature => Some(LocalKind::Never),
                ty => match self.classes.native_ty(ty) {
                    Ok(native) => Some(LocalKind::Value(native)),
                    Err(rejection) => {
                        let what = if is_param {
                            "parameter".to_string()
                        } else if index == 0 {
                            "return".to_string()
                        } else {
                            format!("local {}", Local(index))
                        };
                        let rejection = match rejection {
                            Rejection::Unsupported(reason) => {
                                Rejection::Unsupported(format!("{what} of {reason}"))
                            }
                            Rejection::Invalid(reason) => {
                                Rejection::Invalid(format!("{what} of {reason}"))
                            }
                        };
                        if is_signature {
                            return Err(rejection);
                        }
                        unresolved.push((Local(index), rejection));
                        None
                    }
                },
            };
            self.kinds.push(kind);
        }
        Ok(unresolved)
    }

    /// Give each unresolved local the native type of its definitions, as long
    /// as they agree. Definitions may depend on other unresolved locals, so
    /// this iterates to a fixpoint.
    fn refine(&mut self, mut unresolved: Vec<(Local, Rejection)>) -> Result<(), Rejection> {
        if unresolved.is_empty() {
            return Ok(());
        }
        let mut defs: FxHashMap<Local, Vec<Def<'db, 'db>>> = FxHashMap::default();
        for block in &self.body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(local),
                    value,
                } = &statement.kind
                {
                    defs.entry(*local).or_default().push(Def::Rvalue(value));
                }
            }
            if let Some(terminator) = &block.terminator {
                let destination = match terminator {
                    Terminator::Call { destination, .. }
                    | Terminator::VirtualCall { destination, .. }
                    | Terminator::ShortCircuit { destination, .. }
                    | Terminator::SysOp { destination, .. }
                    | Terminator::Await { destination, .. } => Some(destination),
                    // A `narrow_bind` temporary is typed by `handler_locals`.
                    Terminator::NarrowBind { .. } => None,
                    _ => None,
                };
                if let Some(Place::Local(local)) = destination {
                    defs.entry(*local)
                        .or_default()
                        .push(Def::Terminator(terminator));
                }
            }
        }
        loop {
            let mut progress = false;
            let mut remaining = Vec::with_capacity(unresolved.len());
            for (local, reason) in unresolved {
                let Some(local_defs) = defs.get(&local) else {
                    remaining.push((local, reason));
                    continue;
                };
                let mut agreed: Option<NativeTy<'db>> = None;
                let mut blocked = false;
                for def in local_defs {
                    let ty = match def {
                        Def::Rvalue(value) => self.rvalue_ty(value, None, None),
                        Def::Terminator(terminator) => self.terminator_result_ty(terminator),
                    };
                    match ty {
                        Ok(ty) => match &agreed {
                            Some(previous) if *previous != ty => {
                                return Err(Rejection::unsupported(format!(
                                    "{local} is defined with the native types `{}` and `{}`",
                                    self.describe(previous),
                                    self.describe(&ty)
                                )));
                            }
                            _ => agreed = Some(ty),
                        },
                        Err(Rejection::Invalid(message)) if message.starts_with(UNRESOLVED) => {
                            blocked = true;
                        }
                        Err(other) => return Err(other),
                    }
                }
                match agreed {
                    Some(ty) if !blocked => {
                        self.kinds[local.0] = Some(LocalKind::Value(ty));
                        progress = true;
                    }
                    _ => remaining.push((local, reason)),
                }
            }
            unresolved = remaining;
            if unresolved.is_empty() {
                return Ok(());
            }
            if !progress {
                let (local, reason) = unresolved.swap_remove(0);
                return Err(match reason {
                    Rejection::Unsupported(reason) => Rejection::Unsupported(format!(
                        "{reason} (no definition of {local} refines it)"
                    )),
                    invalid @ Rejection::Invalid(_) => invalid,
                });
            }
        }
    }

    /// Retype the `type_tag` of every union (or nullable union) local as
    /// the tag of that local: a multi-arm `match` on types switches on it,
    /// which is a `match` on the local's variants, so the tag has no value
    /// of its own. Declared `int` in MIR, whose tag numbers are the VM's.
    fn union_tags(&mut self) -> Result<(), Rejection> {
        for block in &self.body.blocks {
            for statement in &block.statements {
                let StatementKind::Assign {
                    destination: Place::Local(destination),
                    value: Rvalue::TypeTag(place),
                } = &statement.kind
                else {
                    continue;
                };
                if matches!(self.kind(*destination)?, LocalKind::Tag(_)) {
                    continue;
                }
                let Place::Local(source) = place else {
                    return Err(Rejection::unsupported(
                        "type tag of a place other than a local",
                    ));
                };
                let LocalKind::Value(ty) = self.kind(*source)? else {
                    return Err(Rejection::unsupported("type tag of a non-value"));
                };
                if ty.union_members().is_none() {
                    return Err(Rejection::unsupported(format!(
                        "type tag of a `{}`",
                        self.describe(ty)
                    )));
                }
                if !matches!(self.kind(*destination)?, LocalKind::Value(NativeTy::Int)) {
                    return Err(Rejection::invalid(format!(
                        "{destination} holds a type tag but is not an int"
                    )));
                }
                self.kinds[destination.0] = Some(LocalKind::Tag(TagSource::Union(*source)));
            }
        }
        Ok(())
    }

    /// Admit the enum of every variant constant the body mentions, so a
    /// later `&self` read of the constant finds its enum registered. (A
    /// variant constant always sits beside a local of its enum type, which
    /// `declare_locals` registered; this makes that an invariant rather
    /// than an assumption.)
    fn register_enum_constants(&mut self) -> Result<(), Rejection> {
        let mut enums = Vec::new();
        for block in &self.body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign { value, .. } = &statement.kind {
                    rvalue_operands(value, &mut |operand| enum_of_constant(operand, &mut enums));
                }
            }
            if let Some(terminator) = &block.terminator {
                terminator_operands(terminator, &mut |operand| {
                    enum_of_constant(operand, &mut enums);
                });
            }
        }
        for enum_ref in enums {
            self.classes
                .check_enum(enum_ref)
                .map_err(|reason| Rejection::unsupported(reason.0))?;
        }
        Ok(())
    }

    fn describe(&self, ty: &NativeTy<'db>) -> String {
        ty.describe(&|decl| self.classes.link_name(decl))
    }

    fn kind(&self, local: Local) -> Result<&LocalKind<'db>, Rejection> {
        match self.kinds.get(local.0) {
            None => Err(Rejection::invalid(format!("{local} is not declared"))),
            Some(None) => Err(Rejection::invalid(format!("{UNRESOLVED} {local}"))),
            Some(Some(kind)) => Ok(kind),
        }
    }

    /// The type of a readable local: `never` has no value and a type value
    /// is only ever a type argument.
    fn local_ty(&self, local: Local) -> Result<NativeTy<'db>, Rejection> {
        match self.kind(local)? {
            LocalKind::Value(ty) => Ok(ty.clone()),
            LocalKind::Never => Err(Rejection::unsupported("read of a `never` local")),
            LocalKind::Type => Err(Rejection::unsupported("type value used as a value")),
            LocalKind::Context => Err(Rejection::unsupported(
                "read of a caught error's `baml.errors.Context` (`catch (e, ctx)`)",
            )),
            LocalKind::Tag(_) => Err(Rejection::unsupported("type tag used as a value")),
        }
    }

    fn place_ty(&self, place: &Place) -> Result<NativeTy<'db>, Rejection> {
        match place {
            Place::Local(local) => self.local_ty(*local),
            Place::Field { base, field } => match self.place_ty(base)? {
                NativeTy::Class(class) => {
                    let info = self.classes.info(class)?;
                    info.fields
                        .get(*field)
                        .map(|f| f.ty.clone())
                        .ok_or_else(|| {
                            Rejection::invalid(format!(
                                "class `{}` has no field slot {field}",
                                info.link_name
                            ))
                        })
                }
                NativeTy::Option(_) => {
                    Err(Rejection::unsupported("field access on a nullable value"))
                }
                // The checker narrowed the union to one class and lowering
                // reads the field by its slot in that class, which the MIR
                // does not name.
                union @ NativeTy::Union(_) => Err(Rejection::unsupported(format!(
                    "field read on a narrowed `{}` (bind it with `let x: C =>` to read its fields)",
                    self.describe(&union)
                ))),
                NativeTy::Thrown => Err(Rejection::unsupported(
                    "field read on a caught error typed by the checker, without a class test (bind it with `let e: C =>`)",
                )),
                other => Err(Rejection::invalid(format!(
                    "field access on a `{}`",
                    self.describe(&other)
                ))),
            },
            Place::Index { base, index, kind } => {
                if *kind == IndexKind::Map {
                    return Err(Rejection::unsupported("map index"));
                }
                let element = match self.place_ty(base)? {
                    NativeTy::Array(element) => *element,
                    NativeTy::Option(_) => {
                        return Err(Rejection::unsupported("index into a nullable value"));
                    }
                    other => {
                        return Err(Rejection::invalid(format!(
                            "index into a `{}`",
                            self.describe(&other)
                        )));
                    }
                };
                if self.local_ty(*index)? != NativeTy::Int {
                    return Err(Rejection::invalid("array index is not an int"));
                }
                Ok(element)
            }
            Place::Capture(_) | Place::Deref(_) => Err(Rejection::unsupported("captured local")),
        }
    }

    /// The type of an operand. `expected` types a `null` constant: it is the
    /// `None` of an `Option` destination, else the unit value.
    fn operand_ty(
        &self,
        operand: &Operand<'db>,
        expected: Option<&NativeTy<'db>>,
    ) -> Result<NativeTy<'db>, Rejection> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place_ty(place),
            Operand::Constant(constant) => match constant {
                Constant::Int(value) => {
                    if Int63::new(*value).is_none() {
                        return Err(Rejection::unsupported(format!(
                            "int literal {value} is outside the int range"
                        )));
                    }
                    Ok(NativeTy::Int)
                }
                Constant::Bool(_) => Ok(NativeTy::Bool),
                Constant::Float(_) => Ok(NativeTy::Float),
                Constant::String(_) => Ok(NativeTy::Str),
                Constant::Null => Ok(match expected {
                    Some(option @ NativeTy::Option(_)) => option.clone(),
                    _ => NativeTy::Null,
                }),
                Constant::Bigint(_) => Ok(NativeTy::Bigint),
                Constant::OmittedArg => Err(Rejection::unsupported("omitted argument")),
                Constant::Function(_) | Constant::GenericFunction { .. } => {
                    Err(Rejection::unsupported("function value"))
                }
                Constant::GlobalItem(_) => Err(Rejection::unsupported("top-level let")),
                Constant::EnumVariant { enum_ref, index } => {
                    let info = self.classes.enum_info(*enum_ref)?;
                    if usize::try_from(*index).is_ok_and(|index| index < info.variants.len()) {
                        Ok(NativeTy::Enum(*enum_ref))
                    } else {
                        Err(Rejection::invalid(format!(
                            "enum `{}` has no variant {index}",
                            info.link_name
                        )))
                    }
                }
            },
        }
    }

    /// A fully realized template as a native type: the element type of an
    /// array literal or the type argument of a generic call.
    fn template_ty(&mut self, template: &TyTemplate) -> Result<NativeTy<'db>, Rejection> {
        let realized = RealizedTy::try_from(template)
            .map_err(|_| Rejection::unsupported("type argument that refers to a type parameter"))?;
        self.classes.native_ty(&RuntimeTy::from(realized))
    }

    /// The template a `reflect.Type` local was loaded with.
    fn type_value(&self, operand: &Operand<'db>) -> Result<&TyTemplate, Rejection> {
        let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = operand
        else {
            return Err(Rejection::unsupported("computed type argument"));
        };
        self.type_values.get(local).ok_or_else(|| {
            Rejection::unsupported(format!("type argument {local} is not a `load_type`"))
        })
    }

    /// The type of an rvalue. `expected` is the destination's type, which
    /// types a `null` constant and says what a union operand the checker
    /// narrowed is read as; `site` records a type test's decision (not
    /// given during refinement, which only asks for the type).
    fn rvalue_ty(
        &mut self,
        value: &Rvalue<'db>,
        expected: Option<&NativeTy<'db>>,
        site: Option<TestSite>,
    ) -> Result<NativeTy<'db>, Rejection> {
        match value {
            Rvalue::Use(operand) => self.operand_ty(operand, expected),
            Rvalue::BinaryOp { op, left, right } => {
                if is_omitted(left) || is_omitted(right) {
                    // The callee prologue's `param == <omitted>` test. Every
                    // call site passes the constant default instead of the
                    // sentinel, so the test is a constant `false`.
                    return match op {
                        BinOp::Eq | BinOp::Ne => Ok(NativeTy::Bool),
                        _ => Err(Rejection::invalid(format!(
                            "`{op}` against an omitted argument"
                        ))),
                    };
                }
                let left_ty = self.operand_ty(left, None)?;
                let right_ty = self.operand_ty(right, Some(&left_ty))?;
                let left_ty = match (&left_ty, &right_ty) {
                    // `x == null` on a nullable: the null constant takes the
                    // other side's type.
                    (NativeTy::Null, NativeTy::Option(_)) => right_ty.clone(),
                    _ => left_ty,
                };
                let with_null = is_null(left) || is_null(right);
                let (left_ty, right_ty) =
                    binop_operand_tys(*op, left_ty, right_ty, expected, with_null);
                binop_ty(*op, &left_ty, &right_ty, with_null).ok_or_else(|| {
                    Rejection::unsupported(format!(
                        "`{op}` on `{}` and `{}`",
                        self.describe(&left_ty),
                        self.describe(&right_ty)
                    ))
                })
            }
            Rvalue::UnaryOp { op, operand } => {
                let ty = self.operand_ty(operand, None)?;
                let ty = unary_operand_ty(ty, expected);
                match (op, &ty) {
                    (UnaryOp::Not, NativeTy::Bool) => Ok(NativeTy::Bool),
                    (UnaryOp::Neg, NativeTy::Int) => Ok(NativeTy::Int),
                    (UnaryOp::Neg, NativeTy::Float) => Ok(NativeTy::Float),
                    (UnaryOp::Neg, NativeTy::Bigint) => Ok(NativeTy::Bigint),
                    (UnaryOp::Truthy, NativeTy::Int | NativeTy::Bool) => Ok(NativeTy::Bool),
                    _ => Err(Rejection::unsupported(format!(
                        "`{op}` on `{}`",
                        self.describe(&ty)
                    ))),
                }
            }
            Rvalue::Array(template, elements) => {
                let element = self.template_ty(template)?;
                for operand in elements {
                    let actual = self.operand_ty(operand, Some(&element))?;
                    if !fits(&actual, &element) {
                        return Err(Rejection::invalid(format!(
                            "array literal of `{}` holds a `{}`",
                            self.describe(&element),
                            self.describe(&actual)
                        )));
                    }
                }
                Ok(NativeTy::Array(Box::new(element)))
            }
            Rvalue::Map(key_template, value_template, entries) => {
                let key = self.template_ty(key_template)?;
                let value = self.template_ty(value_template)?;
                let map = NativeTy::Map(Box::new(key.clone()), Box::new(value.clone()));
                if !matches!(key, NativeTy::Int | NativeTy::Bool | NativeTy::Str) {
                    return Err(Rejection::unsupported(format!(
                        "type `{}`: map key type {}",
                        self.describe(&map),
                        self.describe(&key)
                    )));
                }
                for (key_operand, value_operand) in entries {
                    for (operand, expected) in [(key_operand, &key), (value_operand, &value)] {
                        let actual = self.operand_ty(operand, Some(expected))?;
                        if !fits(&actual, expected) {
                            return Err(Rejection::invalid(format!(
                                "map literal of `{}` holds a `{}`",
                                self.describe(&map),
                                self.describe(&actual)
                            )));
                        }
                    }
                }
                Ok(map)
            }
            Rvalue::Len(place) => match self.place_ty(place)? {
                NativeTy::Array(_) | NativeTy::Map(..) => Ok(NativeTy::Int),
                NativeTy::Str => Err(Rejection::unsupported("length of a string place")),
                other => Err(Rejection::unsupported(format!(
                    "length of a `{}`",
                    self.describe(&other)
                ))),
            },
            Rvalue::Aggregate { kind, fields } => {
                let AggregateKind::Class {
                    class,
                    type_arg_templates,
                } = kind
                else {
                    return Err(Rejection::unsupported("array aggregate"));
                };
                if !type_arg_templates.is_empty() {
                    return Err(Rejection::unsupported("generic class instance"));
                }
                self.classes
                    .check(*class)
                    .map_err(|reason| Rejection::unsupported(reason.0))?;
                let info = self.classes.info(*class)?;
                if fields.len() != info.fields.len() {
                    return Err(Rejection::invalid(format!(
                        "`{}` literal with {} of {} fields",
                        info.link_name,
                        fields.len(),
                        info.fields.len()
                    )));
                }
                let field_tys: Vec<NativeTy<'db>> =
                    info.fields.iter().map(|field| field.ty.clone()).collect();
                let link_name = info.link_name.clone();
                for (operand, field_ty) in fields.iter().zip(&field_tys) {
                    let actual = self.operand_ty(operand, Some(field_ty))?;
                    if !fits(&actual, field_ty) {
                        return Err(Rejection::invalid(format!(
                            "`{link_name}` literal stores a `{}` in a `{}` field",
                            self.describe(&actual),
                            self.describe(field_ty)
                        )));
                    }
                }
                Ok(NativeTy::Class(*class))
            }
            Rvalue::IsType { operand, test } => {
                let ty = self.operand_ty(operand, None)?;
                if ty == NativeTy::Thrown {
                    // A `catch` arm's class test: decided by the class's
                    // name, which needs no native representation of it.
                    return match test {
                        TypeTest::Class { args, .. } if args.is_empty() => Ok(NativeTy::Bool),
                        TypeTest::Class { .. } => {
                            Err(Rejection::unsupported("`catch` arm on a generic class"))
                        }
                        TypeTest::Enum(_) | TypeTest::Template(_) => Err(Rejection::unsupported(
                            "`catch` arm pattern other than a class",
                        )),
                    };
                }
                match (test, &ty) {
                    (
                        TypeTest::Template(TyTemplate::Literal(Literal::Int(_), _)),
                        NativeTy::Int,
                    )
                    | (
                        TypeTest::Template(TyTemplate::Literal(Literal::Bool(_), _)),
                        NativeTy::Bool,
                    )
                    | (
                        TypeTest::Template(TyTemplate::Literal(Literal::String(_), _)),
                        NativeTy::Str,
                    ) => Ok(NativeTy::Bool),
                    (TypeTest::Class { class, args }, NativeTy::Option(_))
                        if args.is_empty() && self.is_done_class(*class) =>
                    {
                        Ok(NativeTy::Bool)
                    }
                    // A test on a union or nullable value selects variants.
                    (_, NativeTy::Union(_) | NativeTy::Option(_)) => {
                        let decided = self.member_test(&ty, test)?;
                        self.record_test(site, decided);
                        Ok(NativeTy::Bool)
                    }
                    _ => Err(Rejection::unsupported(format!(
                        "type test on a `{}` (other than a literal or `baml.iter.Done`)",
                        self.describe(&ty)
                    ))),
                }
            }
            Rvalue::IsTypeTag { operand, tag } => {
                // The coarse tag test lowering emits when the tag alone
                // decides membership: every variant carrying the tag.
                let ty = self.operand_ty(operand, None)?;
                let decided = match &ty {
                    NativeTy::Union(members) => MemberTest::Variants(tag_variants(members, *tag)),
                    NativeTy::Option(inner) => {
                        if *tag == baml_type::typetag::NULL {
                            MemberTest::Null
                        } else {
                            match &**inner {
                                NativeTy::Union(members) => {
                                    MemberTest::Variants(tag_variants(members, *tag))
                                }
                                single => MemberTest::Variants(
                                    (member_tag(single) == Some(*tag))
                                        .then_some(0)
                                        .into_iter()
                                        .collect(),
                                ),
                            }
                        }
                    }
                    other => {
                        return Err(Rejection::unsupported(format!(
                            "type tag test on a `{}`",
                            self.describe(other)
                        )));
                    }
                };
                self.record_test(site, decided);
                Ok(NativeTy::Bool)
            }
            Rvalue::Discriminant(place) => match self.place_ty(place)? {
                NativeTy::Enum(_) => Ok(NativeTy::Int),
                other => Err(Rejection::unsupported(format!(
                    "discriminant of a `{}`",
                    self.describe(&other)
                ))),
            },
            Rvalue::LoadType(_) => {
                Err(Rejection::unsupported("type value stored in a value local"))
            }
            Rvalue::TypeTag(place) => Err(Rejection::unsupported(format!(
                "type tag of a `{}`",
                self.describe(&self.place_ty(place)?)
            ))),
            other => Err(Rejection::unsupported(format!(
                "rvalue {}",
                rvalue_name(other)
            ))),
        }
    }

    fn record_test(&mut self, site: Option<TestSite>, decided: MemberTest) {
        if let Some(site) = site {
            self.member_tests.insert(site, decided);
        }
    }

    /// What `test` decides on a value of the union or nullable type `ty`:
    /// the variants whose values are members of the tested type. The VM
    /// asks whether the value's own type is a subtype of the tested one;
    /// over the closed members here that is which variants the test names.
    fn member_test(
        &mut self,
        ty: &NativeTy<'db>,
        test: &TypeTest<'db>,
    ) -> Result<MemberTest, Rejection> {
        let (members, nullable): (Vec<NativeTy<'db>>, bool) = match ty {
            NativeTy::Union(members) => (members.clone(), false),
            NativeTy::Option(inner) => match &**inner {
                NativeTy::Union(members) => (members.clone(), true),
                single => (vec![single.clone()], true),
            },
            other => {
                return Err(Rejection::invalid(format!(
                    "member test on a `{}`",
                    self.describe(other)
                )));
            }
        };
        let variants_of = |member: &NativeTy<'db>| -> Vec<usize> {
            members
                .iter()
                .enumerate()
                .filter(|(_, candidate)| *candidate == member)
                .map(|(index, _)| index)
                .collect()
        };
        Ok(match test {
            TypeTest::Class { class, args } if args.is_empty() => {
                MemberTest::Variants(variants_of(&NativeTy::Class(*class)))
            }
            TypeTest::Class { .. } => {
                return Err(Rejection::unsupported("type test against a generic class"));
            }
            TypeTest::Enum(enum_ref) => {
                MemberTest::Variants(variants_of(&NativeTy::Enum(*enum_ref)))
            }
            TypeTest::Template(template) => match template {
                TyTemplate::Null => {
                    if nullable {
                        MemberTest::Null
                    } else {
                        MemberTest::Variants(Vec::new())
                    }
                }
                TyTemplate::Literal(literal, _) => {
                    let primitive = match literal {
                        Literal::Int(_) => NativeTy::Int,
                        Literal::Bool(_) => NativeTy::Bool,
                        Literal::String(_) => NativeTy::Str,
                        Literal::Float(_) | Literal::Bigint(_) => {
                            return Err(Rejection::unsupported(
                                "type test against a float or bigint literal",
                            ));
                        }
                    };
                    match variants_of(&primitive).as_slice() {
                        [] => MemberTest::Variants(Vec::new()),
                        [variant] => MemberTest::Literal {
                            variant: *variant,
                            literal: literal.clone(),
                        },
                        _ => return Err(Rejection::invalid("a union member twice")),
                    }
                }
                TyTemplate::EnumVariant(head, name) => {
                    let NativeTy::Enum(enum_ref) =
                        self.template_ty(&TyTemplate::Enum(head.clone()))?
                    else {
                        return Err(Rejection::invalid("variant of a non-enum"));
                    };
                    let info = self.classes.enum_info(enum_ref)?;
                    let index = info
                        .variants
                        .iter()
                        .position(|variant| variant.name == name.as_str())
                        .ok_or_else(|| {
                            Rejection::invalid(format!(
                                "enum `{}` has no variant `{name}`",
                                info.link_name
                            ))
                        })?;
                    match variants_of(&NativeTy::Enum(enum_ref)).as_slice() {
                        [] => MemberTest::Variants(Vec::new()),
                        [variant] => MemberTest::EnumVariant {
                            variant: *variant,
                            index,
                        },
                        _ => return Err(Rejection::invalid("a union member twice")),
                    }
                }
                // `x is int | float`: the variants of every member.
                TyTemplate::Union(tested) => {
                    let mut variants = Vec::new();
                    for member in tested {
                        match self.member_test(ty, &TypeTest::Template(member.clone()))? {
                            MemberTest::Variants(selected) => {
                                for variant in selected {
                                    if !variants.contains(&variant) {
                                        variants.push(variant);
                                    }
                                }
                            }
                            _ => {
                                return Err(Rejection::unsupported(
                                    "type test against a union with a literal or `null` member",
                                ));
                            }
                        }
                    }
                    variants.sort_unstable();
                    MemberTest::Variants(variants)
                }
                other => {
                    let tested = self
                        .template_ty(other)
                        .map_err(|rejection| match rejection {
                            Rejection::Unsupported(reason) => {
                                Rejection::unsupported(format!("type test against {reason}"))
                            }
                            invalid @ Rejection::Invalid(_) => invalid,
                        })?;
                    MemberTest::Variants(variants_of(&tested))
                }
            },
        })
    }

    /// Whether `class` is `baml.iter.Done`, the for-in sentinel.
    fn is_done_class(&self, class: baml_compiler2_hir_ty::extern_loc::ClassRef<'db>) -> bool {
        let head = layout::class_head(self.db, class);
        baml_compiler2_hir::package::spelling(self.db)
            .wire(&head)
            .to_string()
            == "baml.iter.Done"
    }

    /// Type a statement. For an assignment, the result is how the value is
    /// stored into its destination, which the printer applies (a `T` into a
    /// `T | null` place is wrapped in `Some`), and the value's own type.
    fn statement(
        &mut self,
        block: BlockId,
        index: usize,
        kind: &StatementKind<'db>,
    ) -> Result<Option<(Coercion, NativeTy<'db>)>, Rejection> {
        match kind {
            StatementKind::Assign { destination, value } => {
                if let Place::Local(local) = destination {
                    match self.kind(*local)? {
                        LocalKind::Type => {
                            let Rvalue::LoadType(template) = value else {
                                return Err(Rejection::unsupported(
                                    "type value other than a `load_type`",
                                ));
                            };
                            if let Some(previous) = self.type_values.get(local)
                                && previous != template
                            {
                                return Err(Rejection::unsupported(
                                    "type local loaded with two different types",
                                ));
                            }
                            self.type_values.insert(*local, template.clone());
                            return Ok(None);
                        }
                        LocalKind::Never => {
                            return Err(Rejection::unsupported("assignment to a `never` local"));
                        }
                        LocalKind::Context => {
                            return Err(Rejection::unsupported(
                                "assignment to a caught error's `baml.errors.Context`",
                            ));
                        }
                        LocalKind::Tag(source) => {
                            // `_t = type_tag(_e)`: on a thrown value the
                            // class's name, which `handler_locals` checked;
                            // on a union the local itself (`union_tags`).
                            let Rvalue::TypeTag(Place::Local(from)) = value else {
                                return Err(Rejection::invalid(
                                    "type tag assigned from something other than a type tag",
                                ));
                            };
                            if let TagSource::Union(union) = source
                                && union != from
                            {
                                return Err(Rejection::invalid(format!(
                                    "{local} holds the type tag of two locals"
                                )));
                            }
                            return Ok(None);
                        }
                        LocalKind::Value(_) => {}
                    }
                    // The `copy` that seeds a `narrow_bind` temporary with
                    // the caught error: the printer downcasts the error local
                    // in the `narrow_bind` itself and emits no copy.
                    if self.narrows.contains_key(local)
                        && let Rvalue::Use(
                            Operand::Copy(Place::Local(source))
                            | Operand::Move(Place::Local(source)),
                        ) = value
                        && self.local_ty(*source)? == NativeTy::Thrown
                    {
                        if let Some(previous) = self.narrow_sources.insert(*local, *source)
                            && previous != *source
                        {
                            return Err(Rejection::invalid(format!(
                                "{local} narrows two different caught errors"
                            )));
                        }
                        return Ok(None);
                    }
                }
                let expected = self.place_ty(destination)?;
                if is_dead_null_write(value)
                    && !matches!(expected, NativeTy::Null | NativeTy::Option(_))
                {
                    return Ok(None);
                }
                let actual = self.rvalue_ty(
                    value,
                    Some(&expected),
                    Some(TestSite::Statement(block, index)),
                )?;
                let store = coercion(&actual, &expected);
                let allowed = match store {
                    Some(store) if store.is_total() => true,
                    // A value the checker narrowed: the for-in element copy
                    // (the `unknown` result of `next`, now `Option<T>`, into
                    // the `T` loop variable after the `Done` test), a union
                    // read as a member after a type test, a nullable after a
                    // null test. Only a plain copy reads a value that way.
                    Some(_) => matches!(value, Rvalue::Use(_)),
                    None => false,
                };
                if !allowed {
                    return Err(mismatch(
                        &actual,
                        &expected,
                        |ty| self.describe(ty),
                        || {
                            format!(
                                "{destination} has type `{}` but is assigned a `{}`",
                                self.describe(&expected),
                                self.describe(&actual)
                            )
                        },
                    ));
                }
                Ok(store.map(|store| (store, actual)))
            }
            StatementKind::Drop(place) => {
                self.place_ty(place)?;
                Ok(None)
            }
            StatementKind::Nop => Ok(None),
            StatementKind::Intrinsic {
                op: IntrinsicOp::BuiltinTraceHook(_) | IntrinsicOp::ApplyTraceHook,
                ..
            } => Ok(None),
            StatementKind::Intrinsic { .. } => Err(Rejection::unsupported("compiler intrinsic")),
            StatementKind::FreshCell { .. } => Err(Rejection::unsupported("captured local")),
            StatementKind::VirtualFieldStore { .. } => {
                Err(Rejection::unsupported("interface field store"))
            }
        }
    }

    fn terminator(
        &mut self,
        block: BlockId,
        terminator: &Terminator<'db>,
    ) -> Result<(Flow, Option<CallKind<'db>>), Rejection> {
        let flow = match terminator {
            Terminator::Goto { target } => Flow::Goto(target.0),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                self.operand_of(condition, &NativeTy::Bool)?;
                Flow::Branch {
                    then_block: then_block.0,
                    else_block: else_block.0,
                }
            }
            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                ..
            } => {
                let tag = match discriminant {
                    Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                        match self.kind(*local)? {
                            LocalKind::Tag(source) => Some(*source),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if tag.is_none() {
                    self.operand_of(discriminant, &NativeTy::Int)?;
                }
                // The members of the union a tag switch dispatches on.
                let union = match tag {
                    Some(TagSource::Union(local)) => {
                        let ty = self.local_ty(local)?;
                        let members = ty.union_members().map(<[NativeTy<'db>]>::to_vec);
                        Some((members, matches!(ty, NativeTy::Option(_))))
                    }
                    _ => None,
                };
                let mut targets = Vec::with_capacity(arms.len());
                for (index, (key, target)) in arms.iter().enumerate() {
                    match (key, tag) {
                        (SwitchKey::Int(_), None)
                        | (SwitchKey::Class(_), Some(TagSource::Thrown)) => {}
                        (SwitchKey::Class(_), None) => {
                            return Err(Rejection::unsupported("switch on a class tag"));
                        }
                        (SwitchKey::Int(_), Some(TagSource::Thrown)) => {
                            // Primitive types have fixed tags; a thrown
                            // primitive is not native.
                            return Err(Rejection::unsupported("`catch` arm on a primitive type"));
                        }
                        (key, Some(TagSource::Union(_))) => {
                            let (members, nullable) = union.as_ref().expect("a union tag");
                            let decided = match (key, members) {
                                (SwitchKey::Int(tag), _) if *tag == baml_type::typetag::NULL => {
                                    if *nullable {
                                        MemberTest::Null
                                    } else {
                                        MemberTest::Variants(Vec::new())
                                    }
                                }
                                (SwitchKey::Int(tag), Some(members)) => {
                                    MemberTest::Variants(tag_variants(members, *tag))
                                }
                                (SwitchKey::Class(class), Some(members)) => MemberTest::Variants(
                                    members
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, member)| **member == NativeTy::Class(*class))
                                        .map(|(index, _)| index)
                                        .collect(),
                                ),
                                (_, None) => {
                                    return Err(Rejection::invalid(
                                        "type tag switch on a nullable non-union",
                                    ));
                                }
                            };
                            self.member_tests
                                .insert(TestSite::SwitchArm(block, index), decided);
                        }
                    }
                    targets.push(target.0);
                }
                Flow::Switch {
                    arms: targets,
                    otherwise: otherwise.0,
                }
            }
            Terminator::Return | Terminator::Unreachable => Flow::Exit,
            Terminator::Call { target, .. } | Terminator::VirtualCall { target, .. } => {
                let call = self.call(terminator)?;
                return Ok((Flow::Goto(target.0), Some(call)));
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                eval_rhs,
                join,
            } => {
                if *kind == ShortCircuitKind::Coalesce {
                    // `a ?? b`: a non-null `a` is the result and skips `b`.
                    let ty = self.operand_ty(operand, None)?;
                    let NativeTy::Option(inner) = &ty else {
                        return Err(Rejection::unsupported(format!(
                            "`??` on a `{}`",
                            self.describe(&ty)
                        )));
                    };
                    let result = self.place_ty(destination)?;
                    if !stores(inner, &result) {
                        return Err(Rejection::invalid(format!(
                            "`??` stores a `{}` into a `{}`",
                            self.describe(inner),
                            self.describe(&result)
                        )));
                    }
                } else {
                    self.operand_of(operand, &NativeTy::Bool)?;
                    if self.place_ty(destination)? != NativeTy::Bool {
                        return Err(Rejection::invalid(
                            "short-circuit destination is not a bool",
                        ));
                    }
                }
                Flow::Branch {
                    then_block: eval_rhs.0,
                    else_block: join.0,
                }
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                then_block,
                else_block,
            } => {
                let narrows_itself = matches!(
                    source,
                    Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                        if local == destination
                );
                if self.narrows.contains_key(destination) {
                    // A `catch` arm binding (`let e: C => ..`): the temporary
                    // was seeded with the caught error and is narrowed in
                    // place.
                    if !narrows_itself {
                        return Err(Rejection::unsupported(
                            "`catch` binding narrowed from a value other than its own temporary",
                        ));
                    }
                } else {
                    // A binding pattern on a union (`let n: int => ..`): the
                    // temporary holds the union value and the test decides
                    // the variant; what follows reads it narrowed.
                    let source_ty = self.operand_ty(source, None)?;
                    if source_ty.union_members().is_none()
                        && !matches!(source_ty, NativeTy::Option(_))
                    {
                        return Err(Rejection::unsupported(format!(
                            "narrowing pattern on a `{}`",
                            self.describe(&source_ty)
                        )));
                    }
                    let decided = self.member_test(&source_ty, test)?;
                    if !matches!(decided, MemberTest::Variants(_) | MemberTest::Null) {
                        return Err(Rejection::unsupported("narrowing pattern with a literal"));
                    }
                    self.member_tests
                        .insert(TestSite::Terminator(block), decided);
                    if !narrows_itself {
                        let destination_ty = self.local_ty(*destination)?;
                        if coercion(&source_ty, &destination_ty).is_none() {
                            return Err(Rejection::invalid(format!(
                                "{destination} has type `{}` but is bound from a `{}`",
                                self.describe(&destination_ty),
                                self.describe(&source_ty)
                            )));
                        }
                    }
                }
                Flow::Branch {
                    then_block: then_block.0,
                    else_block: else_block.0,
                }
            }
            Terminator::SysOp { .. } => return Err(Rejection::unsupported("sys-op call")),
            Terminator::Spawn { .. } => return Err(Rejection::unsupported("spawn")),
            Terminator::Await { .. } | Terminator::AwaitAny { .. } => {
                return Err(Rejection::unsupported("await"));
            }
            Terminator::Throw { value } => {
                // A class instance, or a caught error thrown on. The VM
                // throws any value; a native throw is always an object.
                let ty = self.operand_ty(value, None)?;
                match ty {
                    NativeTy::Class(_) | NativeTy::Thrown => {}
                    other => {
                        return Err(Rejection::unsupported(format!(
                            "`throw` of a `{}` (only a class instance is thrown natively)",
                            self.describe(&other)
                        )));
                    }
                }
                Flow::Exit
            }
            Terminator::Rethrow { value, context } => {
                self.operand_of(value, &NativeTy::Thrown)?;
                self.context_operand(context)?;
                Flow::Exit
            }
            Terminator::ThrowIfPanic {
                value,
                context,
                otherwise,
            } => {
                self.operand_of(value, &NativeTy::Thrown)?;
                self.context_operand(context)?;
                Flow::Goto(otherwise.0)
            }
        };
        Ok((flow, None))
    }

    /// The context operand of a `rethrow` or `throw_if_panic`: the landing's
    /// context local, which native code carries nowhere.
    fn context_operand(&self, operand: &Operand<'db>) -> Result<(), Rejection> {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                if matches!(self.kind(*local)?, LocalKind::Context) =>
            {
                Ok(())
            }
            _ => Err(Rejection::invalid(
                "rethrow context is not a handler's context local",
            )),
        }
    }

    /// Check that `operand` can be read as an `expected`: as is, coerced, or
    /// narrowed as the checker did (the printer coerces it).
    fn operand_of(
        &self,
        operand: &Operand<'db>,
        expected: &NativeTy<'db>,
    ) -> Result<(), Rejection> {
        let actual = self.operand_ty(operand, Some(expected))?;
        if fits(&actual, expected) {
            Ok(())
        } else {
            Err(mismatch(
                &actual,
                expected,
                |ty| self.describe(ty),
                || {
                    format!(
                        "operand has type `{}` where `{}` is required",
                        self.describe(&actual),
                        self.describe(expected)
                    )
                },
            ))
        }
    }

    /// The type a call or short-circuit stores, for refining its destination.
    fn terminator_result_ty(
        &mut self,
        terminator: &Terminator<'db>,
    ) -> Result<NativeTy<'db>, Rejection> {
        match terminator {
            Terminator::ShortCircuit {
                kind: ShortCircuitKind::Coalesce,
                operand,
                ..
            } => match self.operand_ty(operand, None)? {
                NativeTy::Option(inner) => Ok(*inner),
                other => Err(Rejection::unsupported(format!(
                    "`??` on a `{}`",
                    self.describe(&other)
                ))),
            },
            Terminator::ShortCircuit { .. } => Ok(NativeTy::Bool),
            Terminator::Call { .. } | Terminator::VirtualCall { .. } => {
                match self.call_target(terminator)? {
                    CallKind::Builtin { result, .. } | CallKind::Direct { result, .. } => {
                        Ok(result)
                    }
                    CallKind::Panic(_) => Err(Rejection::unsupported("read of a `never` local")),
                }
            }
            other => Err(Rejection::unsupported(format!(
                "local defined by {}",
                terminator_name(other)
            ))),
        }
    }

    /// Analyze a call, then check its destination.
    fn call(&mut self, terminator: &Terminator<'db>) -> Result<CallKind<'db>, Rejection> {
        let call = self.call_target(terminator)?;
        let (Terminator::Call { destination, .. } | Terminator::VirtualCall { destination, .. }) =
            terminator
        else {
            return Err(Rejection::invalid("not a call"));
        };
        let Place::Local(local) = destination else {
            return Err(Rejection::unsupported("call destination is not a local"));
        };
        match (&call, self.kind(*local)?) {
            // A panic returns before storing anything: its destination may
            // be the `never` temp or, in tail position, the return place.
            (CallKind::Panic(_), LocalKind::Never | LocalKind::Value(_)) => {}
            (CallKind::Panic(_), _) => {
                return Err(Rejection::invalid(
                    "panic destination is not a value or `never` local",
                ));
            }
            (_, LocalKind::Never) => {
                return Err(Rejection::unsupported(
                    "call of a function returning `never`",
                ));
            }
            (_, LocalKind::Type) => {
                return Err(Rejection::unsupported("call storing a type value"));
            }
            (_, LocalKind::Context | LocalKind::Tag(_)) => {
                return Err(Rejection::invalid(
                    "call storing into a handler's context or tag local",
                ));
            }
            (
                CallKind::Builtin { result, .. } | CallKind::Direct { result, .. },
                LocalKind::Value(ty),
            ) if !stores(result, ty) => {
                return Err(Rejection::invalid(format!(
                    "{local} has type `{}` but the call returns `{}`",
                    self.describe(ty),
                    self.describe(result)
                )));
            }
            _ => {}
        }
        Ok(call)
    }

    /// What a call does, without looking at its destination's declared type
    /// (which may be the local being refined).
    fn call_target(&mut self, terminator: &Terminator<'db>) -> Result<CallKind<'db>, Rejection> {
        // Lowering evaluates every argument into a local before the call, so
        // no `RefCell` borrow taken for an argument is live across it. The
        // generated code's `borrow_mut` safety rests on this, so it is
        // checked rather than assumed.
        if let Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. } = terminator
            && let Some(argument) = args.iter().find(|argument| {
                !matches!(
                    argument,
                    Operand::Copy(Place::Local(_))
                        | Operand::Move(Place::Local(_))
                        | Operand::Constant(_)
                )
            })
        {
            return Err(Rejection::invalid(format!(
                "call argument `{argument:?}` is not a local or a constant"
            )));
        }
        match terminator {
            Terminator::Call {
                has_trace,
                argument_layout,
                callee,
                args,
                ntypeargs,
                destination,
                unwind,
                ..
            } => {
                if *has_trace {
                    return Err(Rejection::unsupported("call with a trace attachment"));
                }
                // The unwind edge is the block's (`BasicBlock::unwind`), which
                // the structurer makes explicit.
                let _ = unwind;
                // Lowering has already laid the arguments out in the callee's
                // declared order, named ones included, with `<omitted>` in
                // every slot the call left to a default; the layout the site
                // was checked against adds nothing here.
                let _ = argument_layout;
                let Operand::Constant(Constant::Function(callee)) = callee else {
                    return Err(Rejection::unsupported("indirect call"));
                };
                if *ntypeargs > args.len() {
                    return Err(Rejection::invalid("more type arguments than arguments"));
                }
                let (type_args, value_args) = args.split_at(*ntypeargs);
                let link_name = function_link_name(self.db, *callee);
                if link_name == PANIC_LINK_NAME {
                    let [Operand::Constant(Constant::String(message))] = value_args else {
                        return Err(Rejection::unsupported("panic with a computed message"));
                    };
                    return Ok(CallKind::Panic(message.clone()));
                }
                if let Some(builtin) = self.builtin(&link_name, type_args, value_args)? {
                    return Ok(builtin);
                }
                let callee = match callee {
                    DeclRef::Source(callee) if !self.is_lang_function(*callee) => *callee,
                    _ => {
                        return Err(Rejection::unsupported(format!(
                            "unsupported builtin `{link_name}`"
                        )));
                    }
                };
                if !type_args.is_empty() {
                    return Err(Rejection::unsupported("call with type arguments"));
                }
                let (params, result, defaults) = self.callee_signature(callee, &link_name)?;
                if params.len() != value_args.len() {
                    return Err(Rejection::invalid(format!(
                        "call of `{link_name}` passes {} of {} arguments",
                        value_args.len(),
                        params.len()
                    )));
                }
                let mut substituted = Vec::with_capacity(value_args.len());
                for ((arg, param), default) in value_args.iter().zip(&params).zip(&defaults) {
                    // An omitted argument is the callee's constant default,
                    // passed from here: the callee's own prologue never sees
                    // the sentinel (see `default_constants`).
                    let substitute = if is_omitted(arg) {
                        let Some(constant) = default else {
                            return Err(Rejection::invalid(format!(
                                "call of `{link_name}` omits an argument for a parameter without a default"
                            )));
                        };
                        Some(Operand::Constant(constant.clone()))
                    } else {
                        None
                    };
                    let arg = substitute.as_ref().unwrap_or(arg);
                    substituted.push(
                        substitute
                            .as_ref()
                            .map(|_| default.clone().expect("checked")),
                    );
                    let actual = self.operand_ty(arg, Some(param))?;
                    if !fits(&actual, param) {
                        return Err(mismatch(
                            &actual,
                            param,
                            |ty| self.describe(ty),
                            || {
                                format!(
                                    "call of `{link_name}` passes a `{}` for a `{}` parameter",
                                    self.describe(&actual),
                                    self.describe(param)
                                )
                            },
                        ));
                    }
                }
                if !matches!(destination, Place::Local(_)) {
                    return Err(Rejection::unsupported("call destination is not a local"));
                }
                Ok(CallKind::Direct {
                    callee,
                    args: params,
                    result,
                    substituted,
                })
            }
            Terminator::VirtualCall {
                has_trace,
                argument_layout,
                iface,
                method,
                args,
                ntypeargs,
                self_arg,
                unwind,
                ..
            } => {
                if *has_trace {
                    return Err(Rejection::unsupported("call with a trace attachment"));
                }
                let _ = unwind;
                if *ntypeargs != 0 {
                    return Err(Rejection::unsupported(
                        "interface method call with type arguments",
                    ));
                }
                if argument_layout.as_ref().is_some_and(|layout| {
                    layout.0.len() != args.len() || layout.0.iter().any(Option::is_some)
                }) {
                    return Err(Rejection::unsupported(
                        "call with named or omitted arguments",
                    ));
                }
                let iface_name = baml_compiler2_hir::package::spelling(self.db)
                    .wire(&iface.name)
                    .to_string();
                let receiver = args.get(*self_arg).ok_or_else(|| {
                    Rejection::invalid("virtual call receiver index is out of range")
                })?;
                let receiver_ty = self.operand_ty(receiver, None)?;
                let (builtin, result) = match (iface_name.as_str(), method.as_str(), &receiver_ty) {
                    ("baml.iter.Iterable", "iter", NativeTy::Array(element)) if args.len() == 1 => {
                        (Builtin::Iter, NativeTy::ArrayIter(element.clone()))
                    }
                    ("baml.iter.Iterator", "next", NativeTy::ArrayIter(element))
                        if args.len() == 1 =>
                    {
                        if !matches!(
                            receiver,
                            Operand::Copy(Place::Local(_)) | Operand::Move(Place::Local(_))
                        ) {
                            return Err(Rejection::unsupported(
                                "`next` on an iterator that is not a local",
                            ));
                        }
                        (Builtin::Next, NativeTy::Option(element.clone()))
                    }
                    ("baml.ToString", "to_string", ty) if args.len() == 1 => {
                        if matches!(ty, NativeTy::ArrayIter(_)) {
                            return Err(Rejection::unsupported("`to_string` of an iterator"));
                        }
                        let mut classes = Vec::new();
                        ty.classes(&mut classes);
                        let mut enums = Vec::new();
                        ty.enums(&mut enums);
                        let decls = classes
                            .into_iter()
                            .map(TypeDecl::Class)
                            .chain(enums.into_iter().map(TypeDecl::Enum));
                        if let Some(decl) = decls
                            .into_iter()
                            .find(|decl| self.classes.has_explicit_impl(*decl, &iface.name))
                        {
                            return Err(Rejection::unsupported(format!(
                                "`to_string` on a `{}`: `{}` implements its own `baml.ToString`",
                                self.describe(&receiver_ty),
                                self.classes.link_name(decl)
                            )));
                        }
                        (Builtin::ToStringDefault, NativeTy::Str)
                    }
                    ("baml.Sortable", "sort", NativeTy::Array(element)) if args.len() == 1 => {
                        let kind = match **element {
                            NativeTy::Int => SortKind::Int,
                            NativeTy::Float => SortKind::Float,
                            NativeTy::Str => SortKind::Str,
                            _ => {
                                return Err(Rejection::unsupported(format!(
                                    "`sort` on a `{}` (only primitive arrays sort natively)",
                                    self.describe(&receiver_ty)
                                )));
                            }
                        };
                        (Builtin::Sort(kind), receiver_ty.clone())
                    }
                    _ => {
                        return Err(Rejection::unsupported(format!(
                            "interface method `{iface_name}.{method}` on a `{}`",
                            self.describe(&receiver_ty)
                        )));
                    }
                };
                Ok(CallKind::Builtin { builtin, result })
            }
            _ => Err(Rejection::invalid("not a call")),
        }
    }

    /// The declaration `baml.<namespace>.<name>` of the installed `baml`
    /// package (`baml.ops.Equals`).
    fn lang_decl(&self, namespace: &str, name: &str) -> baml_type::DeclName {
        let root = lang_roots(self.db)
            .get(LangPackage::Baml)
            .expect("the `baml` language package is installed");
        baml_type::DeclName::in_root(
            root,
            vec![baml_type::Name::new(namespace)],
            baml_type::Name::new(name),
        )
    }

    /// Whether `callee` is declared by an installed language package: a
    /// stdlib function the table does not map is reported as such rather
    /// than compiled from its (often generic or intrinsic-laden) body.
    fn is_lang_function(&self, callee: FunctionLoc<'db>) -> bool {
        let root = file_package(self.db, Definition::Function(callee).file(self.db)).root;
        let lang = lang_roots(self.db);
        [
            LangPackage::Baml,
            LangPackage::Reflect,
            LangPackage::Ai,
            LangPackage::Log,
            LangPackage::Trace,
        ]
        .into_iter()
        .any(|package| lang.is(package, root))
    }

    /// The native parameter and return types of a source callee, from its
    /// own lowered signature, and the constant default of each parameter that
    /// has one. Arguments coerce to the parameters (`int` into `int | null`),
    /// which the VM does implicitly and MIR does not spell.
    #[allow(clippy::type_complexity)]
    fn callee_signature(
        &mut self,
        callee: FunctionLoc<'db>,
        link_name: &str,
    ) -> Result<
        (
            Vec<NativeTy<'db>>,
            NativeTy<'db>,
            Vec<Option<Constant<'db>>>,
        ),
        Rejection,
    > {
        let mir = lower_function(self.db, callee, OptLevel::One)
            .as_ref()
            .map_err(|error| {
                Rejection::invalid(format!("MIR lowering of `{link_name}` failed: {error}"))
            })?;
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            return Err(Rejection::unsupported(format!(
                "unsupported builtin `{link_name}`"
            )));
        };
        if body.locals.len() <= mir.arity {
            return Err(Rejection::invalid(format!(
                "`{link_name}` has fewer locals than parameters"
            )));
        }
        let map = |env: &mut Self, ty: &RuntimeTy, what: &str| {
            env.classes
                .native_ty(ty)
                .map_err(|rejection| match rejection {
                    Rejection::Unsupported(reason) => {
                        Rejection::Unsupported(format!("callee `{link_name}` {what} of {reason}"))
                    }
                    invalid @ Rejection::Invalid(_) => invalid,
                })
        };
        let mut params = Vec::with_capacity(mir.arity);
        for local in &body.locals[1..=mir.arity] {
            params.push(map(self, &local.ty, "parameter")?);
        }
        let result = map(self, &body.locals[0].ty, "return")?;
        let data = function_data(self.db, callee);
        let names: Vec<String> = data
            .params
            .iter()
            .map(|param| param.name.to_string())
            .collect();
        let defaulted: Vec<bool> = data.params.iter().map(|param| param.has_default).collect();
        if names.len() != mir.arity {
            return Err(Rejection::invalid(format!(
                "`{link_name}` parameter list disagrees with its MIR arity"
            )));
        }
        let defaults =
            default_constants(body, &names, &defaulted).map_err(|rejection| match rejection {
                Rejection::Unsupported(reason) => {
                    Rejection::Unsupported(format!("callee `{link_name}` {reason}"))
                }
                invalid @ Rejection::Invalid(_) => invalid,
            })?;
        Ok((params, result, defaults))
    }

    /// The `bex_aot` mapping of the stdlib function `link_name`, if it has
    /// one. `None` means the callee is compiled like a user function when it
    /// has source.
    fn builtin(
        &mut self,
        link_name: &str,
        type_args: &[Operand<'db>],
        args: &[Operand<'db>],
    ) -> Result<Option<CallKind<'db>>, Rejection> {
        let arity = |expected_types: usize, expected_values: usize| {
            if type_args.len() == expected_types && args.len() == expected_values {
                Ok(())
            } else {
                Err(Rejection::invalid(format!(
                    "`{link_name}` called with {} type and {} value arguments",
                    type_args.len(),
                    args.len()
                )))
            }
        };
        let (builtin, result) = match link_name {
            "baml.Array.push" => {
                arity(1, 2)?;
                let NativeTy::Array(element) = self.operand_ty(&args[0], None)? else {
                    return Err(Rejection::invalid("`push` on a non-array"));
                };
                self.operand_of(&args[1], &element)?;
                (Builtin::ArrayPush, NativeTy::Int)
            }
            "baml.json.deserialize" => {
                arity(1, 1)?;
                self.operand_of(&args[0], &NativeTy::Str)?;
                let template = self.type_value(&type_args[0])?.clone();
                let target = self.template_ty(&template)?;
                // A literal type erases to its primitive natively, so a
                // decode into one would accept what the VM rejects.
                if let Ok(realized) = RealizedTy::try_from(&template)
                    && let Some(literal) = self.classes.literal_type_in(&RuntimeTy::from(realized))
                {
                    return Err(Rejection::unsupported(format!(
                        "JSON decode into a literal type {literal} (the VM rejects a value outside the literal; the native type is the erased primitive)"
                    )));
                }
                (Builtin::JsonDeserialize(target.clone()), target)
            }
            "baml.json.to_string" => {
                arity(0, 1)?;
                let ty = self.operand_ty(&args[0], None)?;
                if matches!(ty, NativeTy::ArrayIter(_)) {
                    return Err(Rejection::unsupported("JSON of an iterator"));
                }
                (Builtin::JsonToString, NativeTy::Str)
            }
            "baml._to_string_default" => {
                arity(1, 1)?;
                let ty = self.operand_ty(&args[0], None)?;
                if matches!(ty, NativeTy::ArrayIter(_)) {
                    return Err(Rejection::unsupported("`to_string` of an iterator"));
                }
                (Builtin::ToStringDefault, NativeTy::Str)
            }
            "baml.String.length" | "baml.String.char_count" => {
                arity(0, 1)?;
                self.operand_of(&args[0], &NativeTy::Str)?;
                (Builtin::StringLength, NativeTy::Int)
            }
            "baml.String.is_ascii" => {
                arity(0, 1)?;
                self.operand_of(&args[0], &NativeTy::Str)?;
                (Builtin::StringIsAscii, NativeTy::Bool)
            }
            "baml.Float.floor" => {
                arity(0, 1)?;
                self.operand_of(&args[0], &NativeTy::Float)?;
                (Builtin::FloatFloor, NativeTy::Float)
            }
            "baml.Float.itrunc" => {
                arity(0, 1)?;
                self.operand_of(&args[0], &NativeTy::Float)?;
                (Builtin::FloatItrunc, NativeTy::Int)
            }
            "baml.Map.length"
            | "baml.Map.has"
            | "baml.Map.get"
            | "baml.Map.index"
            | "baml.Map.set"
            | "baml.Map.delete"
            | "baml.Map.keys"
            | "baml.Map.values"
            | "baml.Map.get_or_insert"
            | "baml.Map.clear" => {
                let op = match link_name.strip_prefix("baml.Map.") {
                    Some("length") => MapOp::Length,
                    Some("has") => MapOp::Has,
                    Some("get") => MapOp::Get,
                    Some("index") => MapOp::Index,
                    Some("set") => MapOp::Set,
                    Some("delete") => MapOp::Delete,
                    Some("keys") => MapOp::Keys,
                    Some("values") => MapOp::Values,
                    Some("get_or_insert") => MapOp::GetOrInsert,
                    Some("clear") => MapOp::Clear,
                    _ => unreachable!("matched above"),
                };
                let value_count = match op {
                    MapOp::Length | MapOp::Keys | MapOp::Values | MapOp::Clear => 1,
                    MapOp::Has | MapOp::Get | MapOp::Index | MapOp::Delete => 2,
                    MapOp::Set | MapOp::GetOrInsert => 3,
                };
                // A method call carries `<K, V>`; the `set` lowering emits
                // for a map literal's entries carries none.
                if !(type_args.is_empty() || type_args.len() == 2) || args.len() != value_count {
                    return Err(Rejection::invalid(format!(
                        "`{link_name}` called with {} type and {} value arguments",
                        type_args.len(),
                        args.len()
                    )));
                }
                let NativeTy::Map(key, value) = self.operand_ty(&args[0], None)? else {
                    return Err(Rejection::invalid(format!("`{link_name}` on a non-map")));
                };
                if value_count >= 2 {
                    self.operand_of(&args[1], &key)?;
                }
                if value_count == 3 {
                    self.operand_of(&args[2], &value)?;
                }
                let result = match op {
                    MapOp::Length => NativeTy::Int,
                    MapOp::Has => NativeTy::Bool,
                    MapOp::Get | MapOp::Set | MapOp::Delete => NativeTy::Option(value),
                    MapOp::Index | MapOp::GetOrInsert => *value,
                    MapOp::Keys => NativeTy::Array(key),
                    MapOp::Values => NativeTy::Array(value),
                    MapOp::Clear => NativeTy::Null,
                };
                (Builtin::Map(op), result)
            }
            "baml.Bigint.abs" | "baml.Bigint.isqrt" | "baml.Bigint.to_int" => {
                arity(0, 1)?;
                self.operand_of(&args[0], &NativeTy::Bigint)?;
                let (op, result) = match link_name {
                    "baml.Bigint.abs" => (BigintOp::Abs, NativeTy::Bigint),
                    "baml.Bigint.isqrt" => (BigintOp::Isqrt, NativeTy::Bigint),
                    _ => (BigintOp::ToInt, NativeTy::Int),
                };
                (Builtin::Bigint(op), result)
            }
            "baml.Bigint.pow" | "baml.Bigint.ilog" => {
                arity(0, 2)?;
                self.operand_of(&args[0], &NativeTy::Bigint)?;
                self.operand_of(&args[1], &NativeTy::Bigint)?;
                let op = if link_name == "baml.Bigint.pow" {
                    BigintOp::Pow
                } else {
                    BigintOp::Ilog
                };
                (Builtin::Bigint(op), NativeTy::Bigint)
            }
            "baml.Bigint.parse" => {
                arity(0, 1)?;
                self.operand_of(&args[0], &NativeTy::Str)?;
                (Builtin::Bigint(BigintOp::Parse), NativeTy::Bigint)
            }
            "baml.ops.equals_equals" => {
                arity(0, 2)?;
                let operand = match (is_null(&args[0]), is_null(&args[1])) {
                    (false, true) => 0,
                    (true, false) => 1,
                    _ => {
                        let left = self.operand_ty(&args[0], None)?;
                        let right = self.operand_ty(&args[1], None)?;
                        if let (NativeTy::Enum(l), NativeTy::Enum(r)) = (&left, &right)
                            && l == r
                        {
                            return Ok(Some(CallKind::Builtin {
                                builtin: Builtin::EnumEq,
                                result: NativeTy::Bool,
                            }));
                        }
                        // The VM compares the two runtime values whatever
                        // their static types; natively both sides are lifted
                        // into the wider of the two types first, which
                        // changes nothing (a `T` lifted into `T | null` is
                        // still never equal to `null`).
                        let common = [(&left, &right), (&right, &left)]
                            .into_iter()
                            .find(|(from, to)| stores(from, to))
                            .map(|(_, to)| to.clone());
                        let Some(common) = common.filter(baml_eq_supported) else {
                            return Err(Rejection::unsupported(format!(
                                "`==` on a `{}` and a `{}` (only primitives, enums, nullable values and unions of those compare natively through `baml.ops.equals_equals`)",
                                self.describe(&left),
                                self.describe(&right)
                            )));
                        };
                        // The VM dispatches an enum's own `Equals.eq`; the
                        // generated comparison is by variant.
                        let mut enums = Vec::new();
                        common.enums(&mut enums);
                        let equals = self.lang_decl("ops", "Equals");
                        if let Some(enum_ref) = enums.into_iter().find(|enum_ref| {
                            self.classes
                                .has_explicit_impl(TypeDecl::Enum(*enum_ref), &equals)
                        }) {
                            return Err(Rejection::unsupported(format!(
                                "`==` on a `{}`: `{}` implements its own `baml.ops.Equals`",
                                self.describe(&common),
                                self.classes.link_name(TypeDecl::Enum(enum_ref))
                            )));
                        }
                        return Ok(Some(CallKind::Builtin {
                            builtin: Builtin::Equals(common),
                            result: NativeTy::Bool,
                        }));
                    }
                };
                let ty = self.operand_ty(&args[operand], None)?;
                if !matches!(ty, NativeTy::Option(_)) {
                    return Err(Rejection::unsupported(format!(
                        "`== null` on a `{}`",
                        self.describe(&ty)
                    )));
                }
                (Builtin::IsNull { operand }, NativeTy::Bool)
            }
            _ => return Ok(None),
        };
        Ok(Some(CallKind::Builtin { builtin, result }))
    }
}

/// Whether a value of type `actual` can be stored into `expected` without
/// relying on the checker's narrowing.
fn stores(actual: &NativeTy<'_>, expected: &NativeTy<'_>) -> bool {
    coercion(actual, expected).is_some_and(Coercion::is_total)
}

/// Whether an operand of type `actual` can be read as an `expected`: stored
/// as is, or narrowed as the checker did (a union read as a member, a
/// nullable as its value).
fn fits(actual: &NativeTy<'_>, expected: &NativeTy<'_>) -> bool {
    coercion(actual, expected).is_some()
}

/// Whether `ty` implements `bex_aot::BamlEq`, the broad `==`: primitives,
/// `null`, enums (by variant), unions of those and nullable values of
/// those. Classes, arrays and maps compare structurally on the VM, which
/// the runtime does not do.
fn baml_eq_supported(ty: &NativeTy<'_>) -> bool {
    match ty {
        NativeTy::Int | NativeTy::Bool | NativeTy::Float | NativeTy::Bigint | NativeTy::Str => true,
        NativeTy::Null | NativeTy::Enum(_) => true,
        NativeTy::Option(inner) => baml_eq_supported(inner),
        NativeTy::Union(members) => members.iter().all(baml_eq_supported),
        NativeTy::Array(_)
        | NativeTy::Map(..)
        | NativeTy::Class(_)
        | NativeTy::ArrayIter(_)
        | NativeTy::Thrown => false,
    }
}

/// Whether `ty` is a union or a nullable: a value the checker may have
/// narrowed, which an operand then reads as the narrower type.
fn narrowable(ty: &NativeTy<'_>) -> bool {
    matches!(ty, NativeTy::Union(_) | NativeTy::Option(_))
}

/// The types a binary operation's operands are read as. A union or nullable
/// operand the checker narrowed is read as the other operand's type, or,
/// when both sides are narrowed, as the result type `expected` (which an
/// arithmetic operation shares with its operands). `with_null` is the
/// `x == null` test, which is not a narrowing. Shared with the printer, so
/// both sides see one answer.
pub(crate) fn binop_operand_tys<'db>(
    op: BinOp,
    left: NativeTy<'db>,
    right: NativeTy<'db>,
    expected: Option<&NativeTy<'db>>,
    with_null: bool,
) -> (NativeTy<'db>, NativeTy<'db>) {
    if with_null {
        return (left, right);
    }
    let narrow = |from: &NativeTy<'db>, to: &NativeTy<'db>| -> Option<NativeTy<'db>> {
        (narrowable(from) && coercion(from, to).is_some_and(Coercion::narrows)).then(|| to.clone())
    };
    match (narrowable(&left), narrowable(&right)) {
        (true, false) => {
            let left = narrow(&left, &right).unwrap_or(left);
            (left, right)
        }
        (false, true) => {
            let right = narrow(&right, &left).unwrap_or(right);
            (left, right)
        }
        (true, true) => {
            let arithmetic = matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr
            );
            match expected {
                Some(result) if arithmetic => {
                    let left = narrow(&left, result).unwrap_or(left);
                    let right = narrow(&right, result).unwrap_or(right);
                    (left, right)
                }
                _ => (left, right),
            }
        }
        (false, false) => (left, right),
    }
}

/// The type a unary operation's operand is read as: a union or nullable
/// operand the checker narrowed is read as the result type.
pub(crate) fn unary_operand_ty<'db>(
    ty: NativeTy<'db>,
    expected: Option<&NativeTy<'db>>,
) -> NativeTy<'db> {
    match expected {
        Some(result) if narrowable(&ty) && coercion(&ty, result).is_some_and(Coercion::narrows) => {
            result.clone()
        }
        _ => ty,
    }
}

/// A caught error read as a type without a class test is valid BAML the
/// subset cannot type, not a MIR error. Anything else that does not fit is
/// one.
fn mismatch<'db>(
    actual: &NativeTy<'db>,
    expected: &NativeTy<'db>,
    describe: impl Fn(&NativeTy<'db>) -> String,
    what: impl FnOnce() -> String,
) -> Rejection {
    if *actual == NativeTy::Thrown {
        // The checker typed the caught error from the try body's `throws`
        // and lowering reads it as that type without a test. The class is
        // not in the MIR, and a thrown non-class value is not native.
        return Rejection::unsupported(match expected {
            NativeTy::Class(_) => format!(
                "caught error read as a `{0}` without a class test (bind it with `let e: {0} =>`)",
                describe(expected)
            ),
            _ => format!(
                "caught error used as a `{}` (only a class instance is thrown natively)",
                describe(expected)
            ),
        });
    }
    Rejection::invalid(what())
}

fn is_null(operand: &Operand<'_>) -> bool {
    matches!(operand, Operand::Constant(Constant::Null))
}

/// Whether `operand` is the `<omitted>` sentinel of a defaulted argument.
pub(crate) fn is_omitted(operand: &Operand<'_>) -> bool {
    matches!(operand, Operand::Constant(Constant::OmittedArg))
}

fn is_omitted_constant(constant: &Constant<'_>) -> bool {
    matches!(constant, Constant::OmittedArg)
}

/// The constant default of each parameter, parallel to `_1..=arity`; `None`
/// for a parameter without one.
///
/// The VM fills an omitted argument in the callee: a call passes the
/// `<omitted>` sentinel and the callee's prologue tests each defaulted
/// parameter against it (`_t = _k == <omitted>; branch _t -> [fill, next]`,
/// with `fill` storing the default and falling into `next`). Native code has
/// no sentinel value, so the default is passed from the call site instead,
/// which needs it to be a constant: the `fill` block must be a single store
/// of a constant. The prologue stays in the callee, where its test is the
/// constant `false`. A default that is computed (`b: int = a + 1`) is outside
/// the subset, with the parameter named.
pub(crate) fn default_constants<'db>(
    body: &'db MirFunctionBody<'db>,
    param_names: &[String],
    defaulted: &[bool],
) -> Result<Vec<Option<Constant<'db>>>, Rejection> {
    let mut defaults = vec![None; param_names.len()];
    for block in &body.blocks {
        let Some(Terminator::Branch {
            condition: Operand::Copy(Place::Local(test)) | Operand::Move(Place::Local(test)),
            then_block,
            else_block,
        }) = &block.terminator
        else {
            continue;
        };
        let param =
            block
                .statements
                .iter()
                .find_map(|statement| match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(destination),
                        value:
                            Rvalue::BinaryOp {
                                op: BinOp::Eq,
                                left:
                                    Operand::Copy(Place::Local(param))
                                    | Operand::Move(Place::Local(param)),
                                right,
                            },
                    } if destination == test && is_omitted(right) => Some(*param),
                    _ => None,
                });
        let Some(param) = param else {
            continue;
        };
        let Some(name) = param
            .0
            .checked_sub(1)
            .and_then(|index| param_names.get(index))
        else {
            return Err(Rejection::invalid(format!(
                "{param} is tested against an omitted argument but is not a parameter"
            )));
        };
        let not_constant = || {
            Rejection::unsupported(format!(
                "default of parameter `{name}` is not a constant (the callee prologue computes it)"
            ))
        };
        let fill = body.block(*then_block);
        let constant = match (fill.statements.as_slice(), &fill.terminator) {
            (
                [
                    Statement {
                        kind:
                            StatementKind::Assign {
                                destination: Place::Local(destination),
                                value,
                            },
                        ..
                    },
                ],
                Some(Terminator::Goto { target }),
            ) if destination == &param && target == else_block => match value {
                Rvalue::Use(Operand::Constant(constant)) if !is_omitted_constant(constant) => {
                    constant.clone()
                }
                // A negative literal lowers as the negation of a constant.
                Rvalue::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Operand::Constant(constant),
                } => match constant {
                    Constant::Int(value) => Constant::Int(value.wrapping_neg()),
                    Constant::Float(value) => Constant::Float(-value),
                    Constant::Bigint(value) => Constant::Bigint(-value),
                    _ => return Err(not_constant()),
                },
                _ => return Err(not_constant()),
            },
            _ => return Err(not_constant()),
        };
        defaults[param.0 - 1] = Some(constant);
    }
    for (index, (name, has_default)) in param_names.iter().zip(defaulted).enumerate() {
        if *has_default && defaults[index].is_none() {
            return Err(Rejection::invalid(format!(
                "parameter `{name}` has a default but no prologue fills it"
            )));
        }
    }
    Ok(defaults)
}

/// The result type of a binary operation on native operands, if the subset
/// has it. `with_null` says one operand is the `null` constant.
pub(crate) fn binop_ty<'db>(
    op: BinOp,
    left: &NativeTy<'db>,
    right: &NativeTy<'db>,
    with_null: bool,
) -> Option<NativeTy<'db>> {
    use NativeTy::{Bigint, Bool, Float, Int, Null, Option as Opt, Str};
    // A mixed `int` / `bigint` operation widens the `int`, as the stdlib's
    // `Add<int> for bigint` and friends declare.
    let bigint_pair = matches!((left, right), (Bigint, Bigint | Int) | (Int, Bigint));
    match op {
        BinOp::Add => match (left, right) {
            (Int, Int) => Some(Int),
            (Float, Float) => Some(Float),
            (Str, Str) => Some(Str),
            _ if bigint_pair => Some(Bigint),
            _ => None,
        },
        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => match (left, right) {
            (Int, Int) => Some(Int),
            (Float, Float) => Some(Float),
            _ if bigint_pair => Some(Bigint),
            _ => None,
        },
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            match (left, right) {
                (Int, Int) => Some(Int),
                _ if bigint_pair => Some(Bigint),
                _ => None,
            }
        }
        BinOp::Eq | BinOp::Ne => match (left, right) {
            (Int, Int) | (Bool, Bool) | (Float, Float) | (Str, Str) | (Null, Null) => Some(Bool),
            (Bigint, Bigint) => Some(Bool),
            (NativeTy::Enum(l), NativeTy::Enum(r)) if l == r => Some(Bool),
            (Opt(_), Opt(_)) if left == right && with_null => Some(Bool),
            _ => None,
        },
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => match (left, right) {
            (Int, Int) | (Float, Float) | (Str, Str) | (Bigint, Bigint) => Some(Bool),
            _ => None,
        },
    }
}

/// Whether `value` is the `null` an exit edge the checker proved dead writes
/// to a non-nullable local: the fall-through of `while (true)` into an `int`
/// return place. The VM would store the null; the native code treats reaching
/// the write as unreachable, which the checker guarantees it is.
pub(crate) fn is_dead_null_write(value: &Rvalue<'_>) -> bool {
    matches!(value, Rvalue::Use(Operand::Constant(Constant::Null)))
}

fn rvalue_name(value: &Rvalue<'_>) -> &'static str {
    match value {
        Rvalue::TraceHookSettings { .. } => "trace hook settings",
        Rvalue::Use(_) => "use",
        Rvalue::BinaryOp { .. } => "binary op",
        Rvalue::UnaryOp { .. } => "unary op",
        Rvalue::Array(..) => "array literal",
        Rvalue::Uint8Array(_) => "byte array literal",
        Rvalue::Map(..) => "map literal",
        Rvalue::Aggregate { .. } => "class or array construction",
        Rvalue::Discriminant(_) => "discriminant",
        Rvalue::TypeTag(_) => "type tag",
        Rvalue::Len(_) => "length",
        Rvalue::IsType { .. } => "type test",
        Rvalue::IsTypeTag { .. } => "type tag test",
        Rvalue::MakeClosure { .. } => "closure",
        Rvalue::MakeBoundMethod { .. } => "bound method",
        Rvalue::MakeVirtualBoundMethod { .. } => "virtual bound method",
        Rvalue::MakeVirtualFunction { .. } => "virtual function",
        Rvalue::VirtualFieldAccess { .. } => "interface field read",
        Rvalue::MakeGenericFunction { .. } | Rvalue::MakeGenericFunctionFromValue { .. } => {
            "generic function value"
        }
        Rvalue::LoadType(_) => "type value",
        Rvalue::CurrentPackage(_) => "current package",
    }
}

fn terminator_name(terminator: &Terminator<'_>) -> &'static str {
    match terminator {
        Terminator::Goto { .. } => "goto",
        Terminator::Branch { .. } => "branch",
        Terminator::NarrowBind { .. } => "narrowing pattern",
        Terminator::Switch { .. } => "switch",
        Terminator::Return => "return",
        Terminator::Call { .. } => "call",
        Terminator::VirtualCall { .. } => "interface method call",
        Terminator::Unreachable => "unreachable",
        Terminator::SysOp { .. } => "sys-op call",
        Terminator::Spawn { .. } => "spawn",
        Terminator::Await { .. } | Terminator::AwaitAny { .. } => "await",
        Terminator::ShortCircuit { .. } => "short-circuit",
        Terminator::Throw { .. } => "throw",
        Terminator::Rethrow { .. } => "rethrow",
        Terminator::ThrowIfPanic { .. } => "throw-if-panic",
    }
}

// ── Handle aliases ──────────────────────────────────────────────────────────

/// Temporaries that can be read through their source place instead of being
/// declared and assigned.
///
/// Lowering copies an array or class local into a temporary before indexing
/// or a field read (`_8 = copy _1; _7 = _8[_9]`). On the VM that copy is a
/// register move; natively it would clone the handle (an `Rc` increment, and
/// a decrement when the temporary is next overwritten) on every element
/// read, which is the dominant cost of array loops. Such a temporary `t`
/// becomes an alias of its source place `p` when the substitution cannot be
/// observed:
///
/// - `t` is a non-`Copy` value local other than a parameter or the return
///   place, and not an iterator (the only type the printer passes by `&mut`);
/// - `t` is assigned exactly once, by `t = copy p` with `p` a local or a
///   field of a local of the same type (a copy that unwraps a `T | null`
///   into a `T` is a conversion, not a copy), neither of them an alias
///   itself;
/// - every mention of `t` is in the defining block after that assignment,
///   so the alias is read only where it was written;
/// - between the assignment and the last mention nothing assigns `p`'s
///   local, and for a field `p` nothing assigns any class field (the base
///   may have other handles).
///
/// Writes *through* `t` (`t[i] = v`) reach the same object through `p`.
pub(crate) fn find_aliases(
    body: &MirFunctionBody<'_>,
    arity: usize,
    kinds: &[LocalKind<'_>],
    stores: &[Vec<Option<Coercion>>],
) -> FxHashMap<Local, Place> {
    let mut defs = vec![0usize; body.locals.len()];
    // Every local a statement or terminator mentions, per block: one entry
    // per statement, then one for the terminator.
    let mut mentions: Vec<Vec<Vec<Local>>> = Vec::with_capacity(body.blocks.len());
    for block in &body.blocks {
        let mut block_mentions = Vec::with_capacity(block.statements.len() + 1);
        for statement in &block.statements {
            let mut locals = Vec::new();
            if let StatementKind::Assign {
                destination: Place::Local(local),
                ..
            } = &statement.kind
            {
                defs[local.0] += 1;
            }
            if !statement_locals(&statement.kind, &mut locals) {
                return FxHashMap::default();
            }
            block_mentions.push(locals);
        }
        let mut locals = Vec::new();
        if let Some(terminator) = &block.terminator {
            if let Terminator::Call { destination, .. }
            | Terminator::VirtualCall { destination, .. }
            | Terminator::ShortCircuit { destination, .. } = terminator
                && let Place::Local(local) = destination
            {
                defs[local.0] += 1;
            }
            if let Terminator::NarrowBind { destination, .. } = terminator {
                defs[destination.0] += 1;
            }
            if !terminator_locals(terminator, &mut locals) {
                return FxHashMap::default();
            }
        }
        block_mentions.push(locals);
        mentions.push(block_mentions);
    }
    // Whether `local` is mentioned anywhere but after statement `def` of
    // block `block` (the defining statement itself mentions it).
    let mentioned_elsewhere = |local: Local, block: usize, def: usize| {
        mentions.iter().enumerate().any(|(b, block_mentions)| {
            block_mentions
                .iter()
                .enumerate()
                .any(|(i, locals)| (b != block || i < def) && locals.contains(&local))
        })
    };

    let mut candidates: Vec<(Local, Place)> = Vec::new();
    for (b, block) in body.blocks.iter().enumerate() {
        for (i, statement) in block.statements.iter().enumerate() {
            let StatementKind::Assign {
                destination: Place::Local(temp),
                value: Rvalue::Use(Operand::Copy(source) | Operand::Move(source)),
            } = &statement.kind
            else {
                continue;
            };
            let temp = *temp;
            if temp.0 == 0 || temp.0 <= arity || defs[temp.0] != 1 {
                continue;
            }
            if stores[b][i] != Some(Coercion::Identity) {
                continue;
            }
            let Some(LocalKind::Value(ty)) = kinds.get(temp.0) else {
                continue;
            };
            if ty.is_copy() || matches!(ty, NativeTy::ArrayIter(_) | NativeTy::Thrown) {
                continue;
            }
            let (base, is_field) = match source {
                Place::Local(base) => (*base, false),
                Place::Field { base, .. } => match &**base {
                    Place::Local(base) => (*base, true),
                    _ => continue,
                },
                Place::Index { .. } | Place::Capture(_) | Place::Deref(_) => continue,
            };
            if base == temp || mentioned_elsewhere(temp, b, i) {
                continue;
            }
            // The window between the copy and the last read of the alias.
            let block_mentions = &mentions[b];
            let last = block_mentions
                .iter()
                .rposition(|locals| locals.contains(&temp))
                .unwrap_or(i);
            let end = last.min(block.statements.len());
            let window = if end > i {
                &block.statements[i + 1..end]
            } else {
                &[]
            };
            let source_changes = window.iter().any(|statement| match &statement.kind {
                StatementKind::Assign { destination, .. } => match destination {
                    Place::Local(local) => *local == base,
                    Place::Field { .. } => is_field,
                    Place::Index { .. } | Place::Capture(_) | Place::Deref(_) => false,
                },
                _ => false,
            });
            if source_changes {
                continue;
            }
            candidates.push((temp, source.clone()));
        }
    }
    // No chains: an alias of an alias would need the outer window checked
    // against the inner source, and lowering does not produce them.
    let temps: Vec<Local> = candidates.iter().map(|(temp, _)| *temp).collect();
    candidates
        .into_iter()
        .filter(|(_, source)| match source {
            Place::Local(base) => !temps.contains(base),
            Place::Field { base, .. } => {
                !matches!(&**base, Place::Local(base) if temps.contains(base))
            }
            Place::Index { .. } | Place::Capture(_) | Place::Deref(_) => false,
        })
        .collect()
}

fn place_locals(place: &Place, out: &mut Vec<Local>) {
    match place {
        Place::Local(local) => out.push(*local),
        Place::Field { base, .. } => place_locals(base, out),
        Place::Index { base, index, .. } => {
            place_locals(base, out);
            out.push(*index);
        }
        Place::Capture(_) | Place::Deref(_) => {}
    }
}

fn operand_locals(operand: &Operand<'_>, out: &mut Vec<Local>) {
    if let Operand::Copy(place) | Operand::Move(place) = operand {
        place_locals(place, out);
    }
}

/// The locals an admitted rvalue mentions; `false` for a kind the subset
/// does not have, whose operands this does not know.
fn rvalue_locals(value: &Rvalue<'_>, out: &mut Vec<Local>) -> bool {
    match value {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp { operand, .. }
        | Rvalue::IsType { operand, .. }
        | Rvalue::IsTypeTag { operand, .. } => {
            operand_locals(operand, out);
        }
        Rvalue::BinaryOp { left, right, .. } => {
            operand_locals(left, out);
            operand_locals(right, out);
        }
        Rvalue::Array(_, elements)
        | Rvalue::Aggregate {
            fields: elements, ..
        } => {
            for element in elements {
                operand_locals(element, out);
            }
        }
        Rvalue::Map(_, _, entries) => {
            for (key, value) in entries {
                operand_locals(key, out);
                operand_locals(value, out);
            }
        }
        Rvalue::Len(place) | Rvalue::Discriminant(place) | Rvalue::TypeTag(place) => {
            place_locals(place, out);
        }
        Rvalue::LoadType(_) => {}
        _ => return false,
    }
    true
}

/// Record the enum of a variant constant.
fn enum_of_constant<'db>(
    operand: &Operand<'db>,
    out: &mut Vec<baml_compiler2_hir_ty::extern_loc::EnumRef<'db>>,
) {
    if let Operand::Constant(Constant::EnumVariant { enum_ref, .. }) = operand
        && !out.contains(enum_ref)
    {
        out.push(*enum_ref);
    }
}

/// Every operand an rvalue the subset may admit reads. Kinds outside the
/// subset contribute nothing: admission rejects them later.
fn rvalue_operands<'db>(value: &Rvalue<'db>, f: &mut dyn FnMut(&Operand<'db>)) {
    match value {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp { operand, .. }
        | Rvalue::IsType { operand, .. }
        | Rvalue::IsTypeTag { operand, .. } => {
            f(operand);
        }
        Rvalue::BinaryOp { left, right, .. } => {
            f(left);
            f(right);
        }
        Rvalue::Array(_, elements)
        | Rvalue::Aggregate {
            fields: elements, ..
        } => elements.iter().for_each(f),
        Rvalue::Map(_, _, entries) => {
            for (key, value) in entries {
                f(key);
                f(value);
            }
        }
        _ => {}
    }
}

/// Every operand a terminator the subset may admit reads.
fn terminator_operands<'db>(terminator: &Terminator<'db>, f: &mut dyn FnMut(&Operand<'db>)) {
    match terminator {
        Terminator::Branch { condition, .. } => f(condition),
        Terminator::Switch { discriminant, .. } => f(discriminant),
        Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. } => {
            args.iter().for_each(f);
        }
        Terminator::ShortCircuit { operand, .. } => f(operand),
        Terminator::Throw { value } => f(value),
        _ => {}
    }
}

fn statement_locals(kind: &StatementKind<'_>, out: &mut Vec<Local>) -> bool {
    match kind {
        StatementKind::Assign { destination, value } => {
            place_locals(destination, out);
            rvalue_locals(value, out)
        }
        StatementKind::Drop(place) => {
            place_locals(place, out);
            true
        }
        StatementKind::Intrinsic { args, .. } => {
            for argument in args {
                operand_locals(argument, out);
            }
            true
        }
        StatementKind::Nop => true,
        StatementKind::FreshCell { .. } | StatementKind::VirtualFieldStore { .. } => false,
    }
}

fn terminator_locals(terminator: &Terminator<'_>, out: &mut Vec<Local>) -> bool {
    match terminator {
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
        Terminator::Branch { condition, .. } => operand_locals(condition, out),
        Terminator::Switch { discriminant, .. } => operand_locals(discriminant, out),
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            operand_locals(callee, out);
            for argument in args {
                operand_locals(argument, out);
            }
            place_locals(destination, out);
        }
        Terminator::VirtualCall {
            args, destination, ..
        } => {
            for argument in args {
                operand_locals(argument, out);
            }
            place_locals(destination, out);
        }
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => {
            operand_locals(operand, out);
            place_locals(destination, out);
        }
        Terminator::NarrowBind {
            source,
            destination,
            ..
        } => {
            operand_locals(source, out);
            out.push(*destination);
        }
        Terminator::Throw { value } => operand_locals(value, out),
        Terminator::Rethrow { value, context }
        | Terminator::ThrowIfPanic { value, context, .. } => {
            operand_locals(value, out);
            operand_locals(context, out);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{BasicBlock, LocalDecl, Statement};

    use super::*;

    fn local_decl(ty: RuntimeTy) -> LocalDecl {
        LocalDecl {
            name: None,
            ty,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn block(
        id: usize,
        statements: Vec<Statement<'static>>,
        terminator: Terminator<'static>,
    ) -> BasicBlock<'static> {
        let mut block = BasicBlock::new(BlockId(id));
        block.statements = statements;
        block.terminator = Some(terminator);
        block
    }

    fn assign(destination: Place, value: Rvalue<'static>) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign { destination, value },
            span: None,
        }
    }

    fn copy(local: usize) -> Operand<'static> {
        Operand::copy_local(Local(local))
    }

    fn local(index: usize) -> Place {
        Place::Local(Local(index))
    }

    fn index(base: usize, index: usize) -> Place {
        Place::Index {
            base: Box::new(local(base)),
            index: Local(index),
            kind: IndexKind::Array,
        }
    }

    /// `_0: int` return, `_1: int[]` parameter, `_2: int[]` temp, `_3: int` index.
    fn kinds() -> Vec<LocalKind<'static>> {
        vec![
            LocalKind::Value(NativeTy::Int),
            LocalKind::Value(NativeTy::Array(Box::new(NativeTy::Int))),
            LocalKind::Value(NativeTy::Array(Box::new(NativeTy::Int))),
            LocalKind::Value(NativeTy::Int),
        ]
    }

    /// Every assignment stores its own type.
    fn identity_stores(body: &MirFunctionBody<'_>) -> Vec<Vec<Option<Coercion>>> {
        body.blocks
            .iter()
            .map(|block| vec![Some(Coercion::Identity); block.statements.len()])
            .collect()
    }

    #[test]
    fn a_copy_that_unwraps_is_not_an_alias() {
        // _2 = copy _1 where the store unwraps `T | null` into `T`.
        let body = MirFunctionBody {
            blocks: vec![block(
                0,
                vec![
                    assign(local(2), Rvalue::Use(copy(1))),
                    assign(local(0), Rvalue::Use(Operand::Copy(index(2, 3)))),
                ],
                Terminator::Return,
            )],
            entry: BlockId(0),
            locals: locals(),
        };
        let stores = vec![vec![Some(Coercion::Unwrap), Some(Coercion::Identity)]];
        assert!(find_aliases(&body, 1, &kinds(), &stores).is_empty());
    }

    fn locals() -> Vec<LocalDecl> {
        let array = RuntimeTy::List(Box::new(RuntimeTy::Int));
        vec![
            local_decl(RuntimeTy::Int),
            local_decl(array.clone()),
            local_decl(array),
            local_decl(RuntimeTy::Int),
        ]
    }

    #[test]
    fn a_copy_read_once_in_its_block_aliases_its_source() {
        // _2 = copy _1; _3 = 0; _0 = copy _2[_3]; return
        let body = MirFunctionBody {
            blocks: vec![block(
                0,
                vec![
                    assign(local(2), Rvalue::Use(copy(1))),
                    assign(local(3), Rvalue::Use(Operand::Constant(Constant::Int(0)))),
                    assign(local(0), Rvalue::Use(Operand::Copy(index(2, 3)))),
                ],
                Terminator::Return,
            )],
            entry: BlockId(0),
            locals: locals(),
        };
        let aliases = find_aliases(&body, 1, &kinds(), &identity_stores(&body));
        assert_eq!(aliases.get(&Local(2)), Some(&local(1)));
        assert_eq!(aliases.len(), 1, "scalars and parameters never alias");
    }

    #[test]
    fn a_copy_whose_source_is_reassigned_before_the_read_is_kept() {
        // _2 = copy _1; _1 = copy _2; _0 = copy _2[_3]
        let body = MirFunctionBody {
            blocks: vec![block(
                0,
                vec![
                    assign(local(2), Rvalue::Use(copy(1))),
                    assign(local(1), Rvalue::Use(copy(2))),
                    assign(local(0), Rvalue::Use(Operand::Copy(index(2, 3)))),
                ],
                Terminator::Return,
            )],
            entry: BlockId(0),
            locals: locals(),
        };
        assert!(find_aliases(&body, 1, &kinds(), &identity_stores(&body)).is_empty());
    }

    #[test]
    fn a_copy_read_in_another_block_is_kept() {
        // bb0: _2 = copy _1; goto bb1. bb1: _0 = copy _2[_3]; return
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![assign(local(2), Rvalue::Use(copy(1)))],
                    Terminator::Goto { target: BlockId(1) },
                ),
                block(
                    1,
                    vec![assign(local(0), Rvalue::Use(Operand::Copy(index(2, 3))))],
                    Terminator::Return,
                ),
            ],
            entry: BlockId(0),
            locals: locals(),
        };
        assert!(find_aliases(&body, 1, &kinds(), &identity_stores(&body)).is_empty());
    }

    #[test]
    fn binop_typing() {
        use NativeTy::{Bigint, Bool, Float, Int, Str};
        assert_eq!(binop_ty(BinOp::Add, &Str, &Str, false), Some(Str));
        assert_eq!(binop_ty(BinOp::Add, &Bigint, &Bigint, false), Some(Bigint));
        assert_eq!(binop_ty(BinOp::Mul, &Int, &Bigint, false), Some(Bigint));
        assert_eq!(binop_ty(BinOp::Shl, &Bigint, &Int, false), Some(Bigint));
        assert_eq!(binop_ty(BinOp::Lt, &Bigint, &Bigint, false), Some(Bool));
        assert_eq!(binop_ty(BinOp::Eq, &Bigint, &Int, false), None);
        assert_eq!(binop_ty(BinOp::Add, &Bigint, &Float, false), None);
        assert_eq!(binop_ty(BinOp::Add, &Float, &Float, false), Some(Float));
        assert_eq!(binop_ty(BinOp::Lt, &Str, &Str, false), Some(Bool));
        assert_eq!(binop_ty(BinOp::Add, &Int, &Float, false), None);
        assert_eq!(binop_ty(BinOp::BitAnd, &Float, &Float, false), None);
        let nullable = NativeTy::Option(Box::new(Int));
        assert_eq!(binop_ty(BinOp::Eq, &nullable, &nullable, true), Some(Bool));
        assert_eq!(binop_ty(BinOp::Eq, &nullable, &nullable, false), None);
    }
}
