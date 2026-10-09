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
};

/// The link name of the stdlib function whose call is emitted as a panic.
const PANIC_LINK_NAME: &str = "baml.sys.panic";

/// The marker an unresolved local's read reports during refinement.
const UNRESOLVED: &str = "unresolved local";

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
}

impl<'db> LocalKind<'db> {
    pub(crate) fn value(&self) -> Option<&NativeTy<'db>> {
        match self {
            Self::Value(ty) => Some(ty),
            Self::Type | Self::Never => None,
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
    /// `virtual_call iter as baml.iter.Iterable` on an array.
    Iter,
    /// `virtual_call next as baml.iter.Iterator` on an array iterator.
    Next,
    /// `virtual_call sort as baml.Sortable` on a primitive array.
    Sort(SortKind),
    /// `baml.Map.<op>(m, ..)`.
    Map(MapOp),
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
    /// Temporaries the printer reads through their source place instead of
    /// declaring and assigning; see [`find_aliases`].
    pub aliases: FxHashMap<Local, Place>,
    pub structured: Vec<Stmt>,
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
        if block.unwind.is_some() || block.landing.is_some() || block.handling.is_some() {
            return Err(Rejection::unsupported("catch or defer"));
        }
        match &block.terminator {
            Some(Terminator::Spawn { .. }) => return Err(Rejection::unsupported("spawn")),
            Some(Terminator::Await { .. } | Terminator::AwaitAny { .. }) => {
                return Err(Rejection::unsupported("await"));
            }
            Some(Terminator::SysOp { .. }) => return Err(Rejection::unsupported("sys-op call")),
            Some(Terminator::Throw { .. } | Terminator::Rethrow { .. }) => {
                return Err(Rejection::unsupported("throw"));
            }
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
        if block.shielded {
            return Err(Rejection::unsupported("defer body"));
        }
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
    };
    let unresolved = env.declare_locals(mir.arity)?;
    env.register_enum_constants()?;
    env.refine(unresolved)?;
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
    for block in &body.blocks {
        let mut block_stores = Vec::with_capacity(block.statements.len());
        for statement in &block.statements {
            block_stores.push(env.statement(&statement.kind)?);
        }
        stores.push(block_stores);
        let terminator = block.terminator.as_ref().expect("checked above");
        let (flow, call) = env.terminator(terminator)?;
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
    };
    let structured = structurize(&cfg).map_err(|error| Rejection::invalid(error.to_string()))?;
    let aliases = find_aliases(body, mir.arity, &kinds, &stores);

    Ok(Candidate {
        loc,
        link_name,
        mir,
        body,
        kinds,
        param_names,
        calls,
        stores,
        aliases,
        structured,
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
}

impl<'db> Env<'_, 'db> {
    /// Type every local from its declaration. Locals whose declared type has
    /// no native representation are returned (with the reason) for
    /// refinement; a parameter or the return place must be representable.
    fn declare_locals(&mut self, arity: usize) -> Result<Vec<(Local, Rejection)>, Rejection> {
        let mut unresolved = Vec::new();
        for (index, local) in self.body.locals.iter().enumerate() {
            if local.is_captured {
                return Err(Rejection::unsupported("captured local"));
            }
            let is_param = (1..=arity).contains(&index);
            let is_signature = is_param || index == 0;
            let kind = match &local.ty {
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
                    Terminator::NarrowBind { destination, .. } => {
                        return Err(Rejection::unsupported(format!(
                            "narrowing pattern into {destination}"
                        )));
                    }
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
                        Def::Rvalue(value) => self.rvalue_ty(value, None),
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
                Constant::Bigint(_) => Err(Rejection::unsupported("bigint constant")),
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

    fn rvalue_ty(
        &mut self,
        value: &Rvalue<'db>,
        expected: Option<&NativeTy<'db>>,
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
                binop_ty(*op, &left_ty, &right_ty, is_null(left) || is_null(right)).ok_or_else(
                    || {
                        Rejection::unsupported(format!(
                            "`{op}` on `{}` and `{}`",
                            self.describe(&left_ty),
                            self.describe(&right_ty)
                        ))
                    },
                )
            }
            Rvalue::UnaryOp { op, operand } => {
                let ty = self.operand_ty(operand, None)?;
                match (op, &ty) {
                    (UnaryOp::Not, NativeTy::Bool) => Ok(NativeTy::Bool),
                    (UnaryOp::Neg, NativeTy::Int) => Ok(NativeTy::Int),
                    (UnaryOp::Neg, NativeTy::Float) => Ok(NativeTy::Float),
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
                    if !stores(&actual, &element) {
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
                        if !stores(&actual, expected) {
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
                    if !stores(&actual, field_ty) {
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
                match (test, &ty) {
                    (
                        TypeTest::Template(TyTemplate::Literal(Literal::Int(_), _)),
                        NativeTy::Int,
                    )
                    | (
                        TypeTest::Template(TyTemplate::Literal(Literal::Bool(_), _)),
                        NativeTy::Bool,
                    ) => Ok(NativeTy::Bool),
                    (TypeTest::Class { class, args }, NativeTy::Option(_))
                        if args.is_empty() && self.is_done_class(*class) =>
                    {
                        Ok(NativeTy::Bool)
                    }
                    _ => Err(Rejection::unsupported(
                        "type test other than a literal or `baml.iter.Done`",
                    )),
                }
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
            other => Err(Rejection::unsupported(format!(
                "rvalue {}",
                rvalue_name(other)
            ))),
        }
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
    /// `T | null` place is wrapped in `Some`).
    fn statement(&mut self, kind: &StatementKind<'db>) -> Result<Option<Coercion>, Rejection> {
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
                        LocalKind::Value(_) => {}
                    }
                }
                let expected = self.place_ty(destination)?;
                if is_dead_null_write(value)
                    && !matches!(expected, NativeTy::Null | NativeTy::Option(_))
                {
                    return Ok(None);
                }
                let actual = self.rvalue_ty(value, Some(&expected))?;
                let store = coercion(&actual, &expected);
                let allowed = match store {
                    Some(Coercion::Identity | Coercion::Wrap | Coercion::Null) => true,
                    // The for-in element copy: the `unknown` result of `next`,
                    // now `Option<T>`, into the `T` loop variable after the
                    // `Done` test.
                    Some(Coercion::Unwrap) => matches!(value, Rvalue::Use(_)),
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
                Ok(store)
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
                self.operand_of(discriminant, &NativeTy::Int)?;
                let mut targets = Vec::with_capacity(arms.len());
                for (key, target) in arms {
                    let SwitchKey::Int(_) = key else {
                        return Err(Rejection::unsupported("switch on a class tag"));
                    };
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
                    return Err(Rejection::unsupported("`??`"));
                }
                self.operand_of(operand, &NativeTy::Bool)?;
                if self.place_ty(destination)? != NativeTy::Bool {
                    return Err(Rejection::invalid(
                        "short-circuit destination is not a bool",
                    ));
                }
                Flow::Branch {
                    then_block: eval_rhs.0,
                    else_block: join.0,
                }
            }
            Terminator::NarrowBind { .. } => {
                return Err(Rejection::unsupported("narrowing pattern"));
            }
            Terminator::SysOp { .. } => return Err(Rejection::unsupported("sys-op call")),
            Terminator::Spawn { .. } => return Err(Rejection::unsupported("spawn")),
            Terminator::Await { .. } | Terminator::AwaitAny { .. } => {
                return Err(Rejection::unsupported("await"));
            }
            Terminator::Throw { .. } | Terminator::Rethrow { .. } => {
                return Err(Rejection::unsupported("throw"));
            }
            Terminator::ThrowIfPanic { .. } => return Err(Rejection::unsupported("catch")),
        };
        Ok((flow, None))
    }

    fn operand_of(
        &self,
        operand: &Operand<'db>,
        expected: &NativeTy<'db>,
    ) -> Result<(), Rejection> {
        let actual = self.operand_ty(operand, Some(expected))?;
        if actual == *expected {
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
            (CallKind::Panic(_), LocalKind::Never) => {}
            (CallKind::Panic(_), _) => {
                return Err(Rejection::invalid("panic destination is not `never`"));
            }
            (_, LocalKind::Never) => {
                return Err(Rejection::unsupported(
                    "call of a function returning `never`",
                ));
            }
            (_, LocalKind::Type) => {
                return Err(Rejection::unsupported("call storing a type value"));
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
                if unwind.is_some() {
                    return Err(Rejection::unsupported("call inside a catch"));
                }
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
                    if !stores(&actual, param) {
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
                if unwind.is_some() {
                    return Err(Rejection::unsupported("call inside a catch"));
                }
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
                        return Err(Rejection::unsupported(format!(
                            "`==` on a `{}` and a `{}` (only enums and `null` tests compare natively through `baml.ops.equals_equals`)",
                            self.describe(&left),
                            self.describe(&right)
                        )));
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
/// an unwrap.
fn stores(actual: &NativeTy<'_>, expected: &NativeTy<'_>) -> bool {
    coercion(actual, expected).is_some_and(Coercion::is_total)
}

/// A `T | null` where a `T` is required is a value the checker narrowed
/// (after `x != null`, say): valid BAML the subset cannot type, not a MIR
/// error. Anything else that does not fit is one.
fn mismatch<'db>(
    actual: &NativeTy<'db>,
    expected: &NativeTy<'db>,
    describe: impl Fn(&NativeTy<'db>) -> String,
    what: impl FnOnce() -> String,
) -> Rejection {
    if coercion(actual, expected) == Some(Coercion::Unwrap) {
        Rejection::unsupported(format!(
            "narrowed `{}` used as `{}`",
            describe(actual),
            describe(expected)
        ))
    } else {
        Rejection::invalid(what())
    }
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
    use NativeTy::{Bool, Float, Int, Null, Option as Opt, Str};
    match op {
        BinOp::Add => match (left, right) {
            (Int, Int) => Some(Int),
            (Float, Float) => Some(Float),
            (Str, Str) => Some(Str),
            _ => None,
        },
        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => match (left, right) {
            (Int, Int) => Some(Int),
            (Float, Float) => Some(Float),
            _ => None,
        },
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            match (left, right) {
                (Int, Int) => Some(Int),
                _ => None,
            }
        }
        BinOp::Eq | BinOp::Ne => match (left, right) {
            (Int, Int) | (Bool, Bool) | (Float, Float) | (Str, Str) | (Null, Null) => Some(Bool),
            (NativeTy::Enum(l), NativeTy::Enum(r)) if l == r => Some(Bool),
            (Opt(_), Opt(_)) if left == right && with_null => Some(Bool),
            _ => None,
        },
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => match (left, right) {
            (Int, Int) | (Float, Float) | (Str, Str) => Some(Bool),
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
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => "throw",
        Terminator::ThrowIfPanic { .. } => "catch",
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
            if ty.is_copy() || matches!(ty, NativeTy::ArrayIter(_)) {
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
        Rvalue::Use(operand) | Rvalue::UnaryOp { operand, .. } | Rvalue::IsType { operand, .. } => {
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
        Rvalue::Len(place) | Rvalue::Discriminant(place) => place_locals(place, out),
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
        Rvalue::Use(operand) | Rvalue::UnaryOp { operand, .. } | Rvalue::IsType { operand, .. } => {
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
        use NativeTy::{Bool, Float, Int, Str};
        assert_eq!(binop_ty(BinOp::Add, &Str, &Str, false), Some(Str));
        assert_eq!(binop_ty(BinOp::Add, &Float, &Float, false), Some(Float));
        assert_eq!(binop_ty(BinOp::Lt, &Str, &Str, false), Some(Bool));
        assert_eq!(binop_ty(BinOp::Add, &Int, &Float, false), None);
        assert_eq!(binop_ty(BinOp::BitAnd, &Float, &Float, false), None);
        let nullable = NativeTy::Option(Box::new(Int));
        assert_eq!(binop_ty(BinOp::Eq, &nullable, &nullable, true), Some(Bool));
        assert_eq!(binop_ty(BinOp::Eq, &nullable, &nullable, false), None);
    }
}
