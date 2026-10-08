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

use baml_base::LangPackage;
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    item_data::{function_data, method_owner},
    loc::{DeclRef, FunctionLoc},
    package::lang_roots,
};
use baml_compiler2_hir_ty::layout;
use baml_compiler2_mir::{
    AggregateKind, BasicBlock, BinOp, BlockId, Constant, IndexKind, IntrinsicOp, Local,
    MirFunction, MirFunctionBody, MirFunctionKind, Operand, OptLevel, Place, RealizedTy, RuntimeTy,
    Rvalue, ShortCircuitKind, StatementKind, SwitchKey, Terminator, TyTemplate, TypeTest, UnaryOp,
    function_link_name, lower_function,
};
use baml_type::{Int63, Literal};
use rustc_hash::FxHashMap;

use crate::{
    Rejection,
    classes::ClassTable,
    structure::{Cfg, Flow, Stmt, structurize},
    types::{Coercion, NativeTy, coercion},
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

/// A stdlib function mapped to a `bex_lang` call rather than compiled.
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
    /// `virtual_call iter as baml.iter.Iterable` on an array.
    Iter,
    /// `virtual_call next as baml.iter.Iterator` on an array iterator.
    Next,
    /// `virtual_call sort as baml.Sortable` on a primitive array.
    Sort(SortKind),
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
    },
    /// `baml.sys.panic("...")`: returns the panic instead of calling.
    Panic(String),
    /// A `bex_lang` call; `result` is the destination's type.
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
    pub structured: Vec<Stmt>,
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
    if data.params.iter().any(|param| param.has_default) {
        return Err(Rejection::unsupported(
            "default parameter (the callee prologue fills omitted arguments)",
        ));
    }
    if method_owner(db, loc).is_some() {
        return Err(Rejection::unsupported("method"));
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

    let mut env = Env {
        db,
        body,
        kinds: Vec::with_capacity(body.locals.len()),
        type_values: FxHashMap::default(),
        classes,
    };
    let unresolved = env.declare_locals(mir.arity)?;
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
    for block in &body.blocks {
        for statement in &block.statements {
            env.statement(&statement.kind)?;
        }
        let terminator = block.terminator.as_ref().expect("checked above");
        let (flow, call) = env.terminator(terminator)?;
        flows.push(flow);
        if let Some(call) = call {
            calls.push((block.id, call));
        }
    }

    let cfg = Cfg {
        entry: body.entry.0,
        flows,
    };
    check_initialization(body, mir.arity, &cfg)?;
    let structured = structurize(&cfg).map_err(|error| Rejection::invalid(error.to_string()))?;

    Ok(Candidate {
        loc,
        link_name,
        mir,
        body,
        kinds,
        param_names,
        calls,
        structured,
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

    fn describe(&self, ty: &NativeTy<'db>) -> String {
        ty.describe(&|class| self.classes.link_name(class))
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
                Constant::EnumVariant { .. } => Err(Rejection::unsupported("enum value")),
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
            Rvalue::Len(place) => match self.place_ty(place)? {
                NativeTy::Array(_) => Ok(NativeTy::Int),
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

    fn statement(&mut self, kind: &StatementKind<'db>) -> Result<(), Rejection> {
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
                            return Ok(());
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
                    return Ok(());
                }
                let actual = self.rvalue_ty(value, Some(&expected))?;
                let allowed = match coercion(&actual, &expected) {
                    Some(Coercion::Identity | Coercion::Wrap) => true,
                    // The for-in element copy: the `unknown` result of `next`,
                    // now `Option<T>`, into the `T` loop variable after the
                    // `Done` test.
                    Some(Coercion::Unwrap) => matches!(value, Rvalue::Use(_)),
                    None => false,
                };
                if !allowed {
                    return Err(Rejection::invalid(format!(
                        "{destination} has type `{}` but is assigned a `{}`",
                        self.describe(&expected),
                        self.describe(&actual)
                    )));
                }
                Ok(())
            }
            StatementKind::Drop(place) => {
                self.place_ty(place)?;
                Ok(())
            }
            StatementKind::Nop => Ok(()),
            StatementKind::Intrinsic {
                op: IntrinsicOp::BuiltinTraceHook(_) | IntrinsicOp::ApplyTraceHook,
                ..
            } => Ok(()),
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
            Err(Rejection::invalid(format!(
                "operand has type `{}` where `{}` is required",
                self.describe(&actual),
                self.describe(expected)
            )))
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
                if argument_layout.as_ref().is_some_and(|layout| {
                    layout.0.len() != args.len() - ntypeargs || layout.0.iter().any(Option::is_some)
                }) {
                    return Err(Rejection::unsupported(
                        "call with named or omitted arguments",
                    ));
                }
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
                let (params, result) = self.callee_signature(callee, &link_name)?;
                if params.len() != value_args.len() {
                    return Err(Rejection::invalid(format!(
                        "call of `{link_name}` passes {} of {} arguments",
                        value_args.len(),
                        params.len()
                    )));
                }
                for (arg, param) in value_args.iter().zip(&params) {
                    let actual = self.operand_ty(arg, Some(param))?;
                    if !stores(&actual, param) {
                        return Err(Rejection::invalid(format!(
                            "call of `{link_name}` passes a `{}` for a `{}` parameter",
                            self.describe(&actual),
                            self.describe(param)
                        )));
                    }
                }
                if !matches!(destination, Place::Local(_)) {
                    return Err(Rejection::unsupported("call destination is not a local"));
                }
                Ok(CallKind::Direct {
                    callee,
                    args: params,
                    result,
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
                        if let Some(class) = classes
                            .into_iter()
                            .find(|class| self.classes.has_explicit_impl(*class, &iface.name))
                        {
                            return Err(Rejection::unsupported(format!(
                                "`to_string` on a `{}`: class `{}` implements its own `baml.ToString`",
                                self.describe(&receiver_ty),
                                self.classes.link_name(class)
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
    /// own lowered signature. Arguments coerce to the parameters (`int` into
    /// `int | null`), which the VM does implicitly and MIR does not spell.
    fn callee_signature(
        &mut self,
        callee: FunctionLoc<'db>,
        link_name: &str,
    ) -> Result<(Vec<NativeTy<'db>>, NativeTy<'db>), Rejection> {
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
        Ok((params, result))
    }

    /// The `bex_lang` mapping of the stdlib function `link_name`, if it has
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
            "baml.ops.equals_equals" => {
                arity(0, 2)?;
                let operand = match (is_null(&args[0]), is_null(&args[1])) {
                    (false, true) => 0,
                    (true, false) => 1,
                    _ => {
                        return Err(Rejection::unsupported(
                            "`baml.ops.equals_equals` other than a comparison with `null`",
                        ));
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
    matches!(
        coercion(actual, expected),
        Some(Coercion::Identity | Coercion::Wrap)
    )
}

fn is_null(operand: &Operand<'_>) -> bool {
    matches!(operand, Operand::Constant(Constant::Null))
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

// ── Definite initialization ─────────────────────────────────────────────────

/// Reject a read of a local that some path reaches unassigned.
///
/// MIR's own verifier runs only in debug builds. The generated code declares
/// scalar locals zero-initialized and heap locals uninitialized; neither may
/// stand in for a value BAML never assigned, so the check runs here on every
/// build.
fn check_initialization(
    body: &MirFunctionBody<'_>,
    arity: usize,
    cfg: &Cfg,
) -> Result<(), Rejection> {
    let on_entry = initialized_on_entry(body, arity, cfg);
    for (block, state) in body.blocks.iter().zip(&on_entry) {
        let Some(state) = state else {
            continue; // unreachable block: never emitted
        };
        let mut initialized = state.clone();
        let require = |initialized: &[bool], local: Local| {
            if initialized[local.0] {
                Ok(())
            } else {
                Err(Rejection::invalid(format!(
                    "{local} is read in {} before it is assigned on every path",
                    block.id
                )))
            }
        };
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
                    for local in rvalue_reads(value) {
                        require(&initialized, local)?;
                    }
                    match destination {
                        Place::Local(local) => initialized[local.0] = true,
                        other => {
                            for local in place_reads(other) {
                                require(&initialized, local)?;
                            }
                        }
                    }
                }
                StatementKind::Intrinsic { args, .. } => {
                    for local in args.iter().flat_map(operand_reads) {
                        require(&initialized, local)?;
                    }
                }
                _ => {}
            }
        }
        if let Some(terminator) = &block.terminator {
            for local in terminator_reads(terminator) {
                require(&initialized, local)?;
            }
        }
    }
    Ok(())
}

/// The locals a place reads: its base local and every index.
fn place_reads(place: &Place) -> Vec<Local> {
    match place {
        Place::Local(local) => vec![*local],
        Place::Field { base, .. } => place_reads(base),
        Place::Index { base, index, .. } => {
            let mut reads = place_reads(base);
            reads.push(*index);
            reads
        }
        Place::Capture(_) | Place::Deref(_) => Vec::new(),
    }
}

fn operand_reads(operand: &Operand<'_>) -> Vec<Local> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_reads(place),
        Operand::Constant(_) => Vec::new(),
    }
}

fn rvalue_reads(value: &Rvalue<'_>) -> Vec<Local> {
    match value {
        Rvalue::Use(operand) | Rvalue::UnaryOp { operand, .. } | Rvalue::IsType { operand, .. } => {
            operand_reads(operand)
        }
        Rvalue::BinaryOp { left, right, .. } => {
            let mut reads = operand_reads(left);
            reads.extend(operand_reads(right));
            reads
        }
        Rvalue::Array(_, elements)
        | Rvalue::Aggregate {
            fields: elements, ..
        } => elements.iter().flat_map(operand_reads).collect(),
        Rvalue::Len(place) | Rvalue::Discriminant(place) | Rvalue::TypeTag(place) => {
            place_reads(place)
        }
        _ => Vec::new(),
    }
}

fn terminator_reads(terminator: &Terminator<'_>) -> Vec<Local> {
    match terminator {
        Terminator::Branch { condition, .. } => operand_reads(condition),
        Terminator::Switch { discriminant, .. } => operand_reads(discriminant),
        Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. } => {
            args.iter().flat_map(operand_reads).collect()
        }
        Terminator::ShortCircuit { operand, .. } => operand_reads(operand),
        _ => Vec::new(),
    }
}

/// The locals assigned on every path into each block (`None` for a block no
/// path reaches). Parameters are assigned on entry. A call's destination is
/// assigned on its continuation edge and a short-circuit's on its join edge:
/// both paths into the join assign it, the short one on the edge itself.
fn initialized_on_entry(
    body: &MirFunctionBody<'_>,
    arity: usize,
    cfg: &Cfg,
) -> Vec<Option<Vec<bool>>> {
    let locals = body.locals.len();
    let edges: Vec<Vec<(usize, Option<Local>)>> = body
        .blocks
        .iter()
        .map(|block| edge_writes(block, cfg))
        .collect();
    let writes: Vec<Vec<Local>> = body
        .blocks
        .iter()
        .map(|block| {
            block
                .statements
                .iter()
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        ..
                    } => Some(*local),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let mut entry = vec![false; locals];
    entry[1..=arity].fill(true);
    let mut states: Vec<Option<Vec<bool>>> = vec![None; body.blocks.len()];
    states[body.entry.0] = Some(entry);
    let mut work = vec![body.entry.0];
    while let Some(index) = work.pop() {
        let mut after = states[index]
            .clone()
            .expect("only reached blocks are queued");
        for local in &writes[index] {
            after[local.0] = true;
        }
        for (target, write) in &edges[index] {
            let mut edge = after.clone();
            if let Some(local) = write {
                edge[local.0] = true;
            }
            let merged = match &states[*target] {
                None => edge,
                Some(old) => old.iter().zip(&edge).map(|(a, b)| *a && *b).collect(),
            };
            if states[*target].as_ref() != Some(&merged) {
                states[*target] = Some(merged);
                work.push(*target);
            }
        }
    }
    states
}

/// Each successor of `block` with the local assigned on the way there.
fn edge_writes(block: &BasicBlock<'_>, cfg: &Cfg) -> Vec<(usize, Option<Local>)> {
    let destination = |place: &Place| match place {
        Place::Local(local) => Some(*local),
        _ => None,
    };
    match (&block.terminator, &cfg.flows[block.id.0]) {
        (
            Some(
                Terminator::Call { destination: d, .. }
                | Terminator::VirtualCall { destination: d, .. },
            ),
            Flow::Goto(target),
        ) => {
            vec![(*target, destination(d))]
        }
        (
            Some(Terminator::ShortCircuit { destination: d, .. }),
            Flow::Branch {
                then_block,
                else_block,
            },
        ) => vec![(*then_block, None), (*else_block, destination(d))],
        (_, flow) => match flow {
            Flow::Goto(target) => vec![(*target, None)],
            Flow::Branch {
                then_block,
                else_block,
            } => vec![(*then_block, None), (*else_block, None)],
            Flow::Switch { arms, otherwise } => arms
                .iter()
                .chain([otherwise])
                .map(|target| (*target, None))
                .collect(),
            Flow::Exit => Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{LocalDecl, Statement};

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

    fn assign(local: usize, value: Rvalue<'static>) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(local)),
                value,
            },
            span: None,
        }
    }

    fn copy(local: usize) -> Operand<'static> {
        Operand::copy_local(Local(local))
    }

    fn cfg_of(body: &MirFunctionBody<'static>) -> Cfg {
        let flows = body
            .blocks
            .iter()
            .map(
                |block| match block.terminator.as_ref().expect("terminated") {
                    Terminator::Goto { target } => Flow::Goto(target.0),
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => Flow::Branch {
                        then_block: then_block.0,
                        else_block: else_block.0,
                    },
                    Terminator::ShortCircuit { eval_rhs, join, .. } => Flow::Branch {
                        then_block: eval_rhs.0,
                        else_block: join.0,
                    },
                    Terminator::Return => Flow::Exit,
                    other => panic!("unexpected terminator {other:?}"),
                },
            )
            .collect();
        Cfg {
            entry: body.entry.0,
            flows,
        }
    }

    #[test]
    fn read_assigned_on_one_path_only_is_invalid() {
        // _0 ret, _1 param (bool), _2 int assigned only on the then path.
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![],
                    Terminator::Branch {
                        condition: copy(1),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                ),
                block(
                    1,
                    vec![assign(2, Rvalue::Use(Operand::Constant(Constant::Int(1))))],
                    Terminator::Goto { target: BlockId(2) },
                ),
                block(
                    2,
                    vec![assign(0, Rvalue::Use(copy(2)))],
                    Terminator::Goto { target: BlockId(3) },
                ),
                block(3, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                local_decl(RuntimeTy::Int),
                local_decl(RuntimeTy::Bool),
                local_decl(RuntimeTy::Int),
            ],
        };
        let cfg = cfg_of(&body);
        let error = check_initialization(&body, 1, &cfg).unwrap_err();
        assert!(
            matches!(&error, Rejection::Invalid(reason) if reason.contains("_2 is read in bb2")),
            "{error}"
        );
    }

    #[test]
    fn short_circuit_destination_is_assigned_on_both_paths() {
        // _0 ret bool, _1 param bool, _2 destination.
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![],
                    Terminator::ShortCircuit {
                        operand: copy(1),
                        kind: ShortCircuitKind::And,
                        destination: Place::Local(Local(2)),
                        eval_rhs: BlockId(1),
                        join: BlockId(2),
                    },
                ),
                block(
                    1,
                    vec![assign(2, Rvalue::Use(copy(1)))],
                    Terminator::Goto { target: BlockId(2) },
                ),
                block(
                    2,
                    vec![assign(0, Rvalue::Use(copy(2)))],
                    Terminator::Goto { target: BlockId(3) },
                ),
                block(3, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                local_decl(RuntimeTy::Bool),
                local_decl(RuntimeTy::Bool),
                local_decl(RuntimeTy::Bool),
            ],
        };
        let cfg = cfg_of(&body);
        check_initialization(&body, 1, &cfg).expect("join sees the destination assigned");
        // Without the assignment in the rhs block the join is not covered.
        let mut broken = body.clone();
        broken.blocks[1].statements.clear();
        assert!(matches!(
            check_initialization(&broken, 1, &cfg),
            Err(Rejection::Invalid(_))
        ));
    }

    #[test]
    fn index_write_reads_its_base_and_index() {
        // _0 ret int, _1 param int[], _2 int index never assigned.
        let body = MirFunctionBody {
            blocks: vec![block(
                0,
                vec![Statement {
                    kind: StatementKind::Assign {
                        destination: Place::Index {
                            base: Box::new(Place::Local(Local(1))),
                            index: Local(2),
                            kind: IndexKind::Array,
                        },
                        value: Rvalue::Use(Operand::Constant(Constant::Int(1))),
                    },
                    span: None,
                }],
                Terminator::Return,
            )],
            entry: BlockId(0),
            locals: vec![
                local_decl(RuntimeTy::Int),
                local_decl(RuntimeTy::List(Box::new(RuntimeTy::Int))),
                local_decl(RuntimeTy::Int),
            ],
        };
        let cfg = cfg_of(&body);
        let error = check_initialization(&body, 1, &cfg).unwrap_err();
        assert!(
            matches!(&error, Rejection::Invalid(reason) if reason.contains("_2 is read in bb0")),
            "{error}"
        );
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
