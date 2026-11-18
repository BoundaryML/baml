//! The types native code can hold today, how values of them are represented,
//! and the typing rules of the operators over them.
//!
//! Both the support check and code generation go through these functions, so
//! a construct is accepted exactly when code can be generated for it.

use std::cell::Cell;

use baml_compiler2_hir::loc::DeclRef;
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    layout::{
        class_fields, class_generic_params, class_head, class_ref_of, enum_ref_of, interface_ref_of,
    },
};
use baml_compiler2_mir::{
    AggregateKind, BinOp, CellId, Constant, IndexKind, Landing, Local, MirFunction,
    MirFunctionBody, Operand, Place, RuntimeTy, Rvalue, StatementKind, Terminator, TyTemplate,
    UnaryOp, resolved_aliases_for_package,
};
use baml_type::Literal;

use crate::{
    Blocker,
    check::{Callee, call_target, function_signature},
    flow::terminator_destination,
    loops::ArrayLoops,
    program::{Entry, FnKey},
};

/// Parameter and result types of a function with a native calling convention.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Signature<'db> {
    pub(crate) params: Vec<NativeTy<'db>>,
    pub(crate) result: NativeTy<'db>,
}

/// What a value is to the machine: which register class it travels in and
/// whether the collector follows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Repr {
    Int,
    Float,
    Byte,
    Ref,
    Pair,
}

/// A type whose values native code knows the representation of. There are no
/// tags: the static type alone says what the bits mean.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum NativeTy<'db> {
    /// A signed 64-bit integer.
    Int,
    Float,
    Bool,
    /// The one value of the unit type. It carries no information, so it is
    /// never kept in a variable.
    Null,
    /// A variant of this enum: its position in the declaration, as an int.
    Enum(EnumRef<'db>),
    /// A reference to an immutable string object.
    String,
    /// A reference to an instance of a class at these type arguments.
    /// Instantiations whose arguments are the same under [`type_argument`]
    /// are laid out alike and share a type id.
    Class(ClassRef<'db>, Vec<TypeArg<'db>>),
    /// A reference to a tuple of these element types: an object with one
    /// slot per element, two for an optional scalar, as a class instance
    /// has for its fields.
    Tuple(Vec<NativeTy<'db>>),
    /// A reference to a growable array of the element type.
    Array(Box<NativeTy<'db>>),
    /// A reference to a map from the first type to the second. A key is a
    /// `string`, an `int`, a `bool`, an enum variant or a `float`, and a
    /// value is one slot.
    Map(Box<NativeTy<'db>>, Box<NativeTy<'db>>),
    /// A reference to a closure: a function value. Calling it passes the
    /// closure and then arguments of these types.
    Function(Box<Signature<'db>>),
    /// A reference to the cell holding a local that a closure captures.
    /// Never the type of a source value: only of the local itself.
    Cell(Box<NativeTy<'db>>),
    /// One of these types, none of them a union and at most one `null`: a
    /// reference, zero for `null`, whose object's header says which. An
    /// `int`, `float`, `bool` or enum variant is in a box.
    Union(Vec<NativeTy<'db>>),
    /// Any value at all, in the representation of a union.
    Unknown,
    /// A type or `null`. For a reference type it is the same reference,
    /// with zero for `null`. For an `int`, `float` or `bool` it is a pair of
    /// words: the value's bits, and whether there is a value at all.
    Optional(Box<NativeTy<'db>>),
    /// A reference to the future of a task that returns a value of this
    /// type, which takes one slot.
    Future(Box<NativeTy<'db>>),
}

impl NativeTy<'_> {
    /// Whether a value is a pointer the collector must know about.
    pub(crate) fn is_ref(&self) -> bool {
        match self {
            Self::String
            | Self::Class(..)
            | Self::Tuple(_)
            | Self::Array(_)
            | Self::Map(..)
            | Self::Function(_)
            | Self::Cell(_)
            | Self::Future(_)
            | Self::Union(_)
            | Self::Unknown => true,
            Self::Optional(inner) => inner.is_ref(),
            Self::Int | Self::Float | Self::Bool | Self::Null | Self::Enum(_) => false,
        }
    }

    /// Whether a value says what it is in its object's header: a union or
    /// `unknown`.
    pub(crate) fn is_boxed(&self) -> bool {
        matches!(self, Self::Union(_) | Self::Unknown)
    }

    /// Whether a value is an optional scalar: a pair of words.
    pub(crate) fn is_pair(&self) -> bool {
        matches!(self, Self::Optional(inner) if !inner.is_ref())
    }

    pub(crate) fn repr(&self) -> Repr {
        match self {
            Self::Int | Self::Enum(_) => Repr::Int,
            Self::Float => Repr::Float,
            Self::Bool | Self::Null => Repr::Byte,
            _ if self.is_pair() => Repr::Pair,
            _ => Repr::Ref,
        }
    }

    /// How many slots a value takes in an object.
    pub(crate) fn slots(&self) -> usize {
        if self.is_pair() { 2 } else { 1 }
    }
}

/// A type argument as compiled code is keyed by it: `None` for a type with
/// no native representation, which a function may still be given as long
/// as it holds no value of it (the type of what it throws, say).
pub(crate) type TypeArg<'db> = Option<NativeTy<'db>>;

/// A type as a type argument: every reference type is `unknown`, since code
/// that only moves a reference is the same for all of them. An `int`,
/// `float`, `bool`, enum or optional scalar stays itself: it travels in its
/// own registers, and an enum's box names the enum.
pub(crate) fn type_argument(ty: Option<NativeTy<'_>>) -> TypeArg<'_> {
    ty.map(|ty| if ty.is_ref() { NativeTy::Unknown } else { ty })
}

/// `RuntimeTy` and `TyTemplate` are one family of types with the same
/// variant names; the native type of either is found the same way, apart
/// from how each names a type parameter.
/// Whether the native runtime hashes and compares keys of this type
/// itself: every other key type has a `hash` and an `equals` to call.
pub(crate) fn is_map_key(ty: &NativeTy<'_>) -> bool {
    matches!(
        ty,
        NativeTy::String | NativeTy::Int | NativeTy::Bool | NativeTy::Enum(_) | NativeTy::Float
    )
}

macro_rules! native_type_of {
    ($name:ident, $family:ident, $parameter:pat => $slot:expr) => {
        pub(crate) fn $name<'db>(
            db: &'db dyn baml_compiler2_mir::Db,
            ty: &$family,
            arguments: &[TypeArg<'db>],
        ) -> Option<NativeTy<'db>> {
            Some(match ty {
                $family::Int | $family::Literal(Literal::Int(_), _) => NativeTy::Int,
                $family::Float | $family::Literal(Literal::Float(_), _) => NativeTy::Float,
                $family::Bool | $family::Literal(Literal::Bool(_), _) => NativeTy::Bool,
                $family::Null => NativeTy::Null,
                $family::Enum(head) | $family::EnumVariant(head, _) => {
                    NativeTy::Enum(enum_ref_of(db, head)?)
                }
                // A value of an interface type is any implementor: a
                // reference whose header says which.
                $family::Unknown | $family::Interface(..) => NativeTy::Unknown,
                // A `json` is any of its arms, as a union holds them.
                $family::TypeAlias(head) if crate::json::is_json_alias(db, head) => {
                    NativeTy::Unknown
                }
                $family::String | $family::Literal(Literal::String(_), _) => NativeTy::String,
                $parameter => arguments.get($slot)?.clone()?,
                // `self` in an interface's default body is declared with
                // the interface's own name: any implementor.
                $family::Class(head, _) if interface_ref_of(db, head).is_some() => {
                    NativeTy::Unknown
                }
                // `self` in a method of the standard library's `Array` is
                // declared with the class's name: the array itself, whose
                // element type is the method's first type parameter where
                // the declaration does not say.
                $family::Class(head, class_arguments)
                    if class_arguments.len() <= 1
                        && head.name().as_str() == "Array"
                        && head.namespace().is_empty()
                        && head.key().kind(db) == baml_base::SourceRootKind::Stdlib =>
                {
                    let element = match class_arguments.first() {
                        Some(element) => $name(db, element, arguments)?,
                        None => arguments.first()?.clone()?,
                    };
                    if element.is_pair() {
                        return None;
                    }
                    NativeTy::Array(Box::new(element))
                }
                $family::Class(head, class_arguments) => {
                    let class = class_ref_of(db, head)?;
                    // Every type parameter has its argument, or there is no
                    // layout.
                    if class_generic_params(db, class).len() != class_arguments.len() {
                        return None;
                    }
                    NativeTy::Class(
                        class,
                        class_arguments
                            .iter()
                            .map(|argument| $name(db, argument, arguments))
                            .collect(),
                    )
                }
                $family::Tuple(elements) => NativeTy::Tuple(
                    elements
                        .iter()
                        .map(|element| $name(db, element, arguments))
                        .collect::<Option<_>>()?,
                ),
                $family::List(element) => {
                    let element = $name(db, element, arguments)?;
                    // Elements are one slot each so far.
                    if element.is_pair() {
                        return None;
                    }
                    NativeTy::Array(Box::new(element))
                }
                $family::Map { key, value, .. } => {
                    let key = $name(db, key, arguments)?;
                    let value = $name(db, value, arguments)?;
                    if !is_map_key(&key) || value.is_pair() || value == NativeTy::Null {
                        return None;
                    }
                    NativeTy::Map(Box::new(key), Box::new(value))
                }
                // What the task throws is no part of how its future is
                // held.
                $family::Future(value, _) => {
                    let value = $name(db, value, arguments)?;
                    if value.is_pair() {
                        return None;
                    }
                    NativeTy::Future(Box::new(value))
                }
                $family::Function { params, ret, .. } => {
                    let params = params
                        .iter()
                        .map(|param| $name(db, &param.ty, arguments))
                        .collect::<Option<_>>()?;
                    let result = $name(db, ret, arguments)?;
                    NativeTy::Function(Box::new(Signature { params, result }))
                }
                $family::Union(members) => {
                    // A `bigint` has no native representation yet, so no
                    // value of the union is one; the others are held as
                    // any value is, which is what they will be when one
                    // can be.
                    if members
                        .iter()
                        .any(|member| matches!(member, $family::Bigint))
                    {
                        return Some(NativeTy::Unknown);
                    }
                    let mut flat = Vec::new();
                    for member in members {
                        // Native code makes no `bigint`, so no value of
                        // the union is one: a `json` is every other arm.
                        if matches!(member, $family::Bigint) {
                            continue;
                        }
                        let members = match $name(db, member, arguments)? {
                            NativeTy::Unknown => return Some(NativeTy::Unknown),
                            NativeTy::Union(members) => members,
                            NativeTy::Optional(inner) => vec![*inner, NativeTy::Null],
                            other => vec![other],
                        };
                        for member in members {
                            if !flat.contains(&member) {
                                flat.push(member);
                            }
                        }
                    }
                    match &flat[..] {
                        [only] => only.clone(),
                        // One type or `null` needs no header to tell which.
                        [NativeTy::Null, inner] | [inner, NativeTy::Null] => {
                            NativeTy::Optional(Box::new(inner.clone()))
                        }
                        _ => NativeTy::Union(flat),
                    }
                }
                _ => return None,
            })
        }
    };
}

native_type_of!(native_ty, RuntimeTy, RuntimeTy::TypeVar(parameter) => parameter.index() as usize);
native_type_of!(template_ty, TyTemplate, TyTemplate::TypeArgRef(slot) => *slot as usize);

/// Whether a type names a type parameter anywhere native code looks.
fn names_type_parameter(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::TypeVar(_) => true,
        RuntimeTy::Class(_, arguments) => arguments.iter().any(names_type_parameter),
        RuntimeTy::List(element) => names_type_parameter(element),
        RuntimeTy::Tuple(elements) => elements.iter().any(names_type_parameter),
        RuntimeTy::Union(members) => members.iter().any(names_type_parameter),
        RuntimeTy::Function { params, ret, .. } => {
            names_type_parameter(ret) || params.iter().any(|param| names_type_parameter(&param.ty))
        }
        _ => false,
    }
}

/// A coarse name for a type, for grouping what blocks compilation.
pub(crate) fn type_kind(ty: &RuntimeTy) -> &'static str {
    match ty {
        RuntimeTy::Int => "int",
        RuntimeTy::Float => "float",
        RuntimeTy::Bool => "bool",
        RuntimeTy::Null => "null",
        RuntimeTy::Bigint | RuntimeTy::Literal(Literal::Bigint(_), _) => "bigint",
        RuntimeTy::String | RuntimeTy::Literal(Literal::String(_), _) => "string",
        RuntimeTy::Media(..) => "media",
        RuntimeTy::Literal(..) => "literal",
        RuntimeTy::Class(_, arguments) if arguments.is_empty() => "class",
        RuntimeTy::Tuple(_) => "tuple",
        RuntimeTy::Class(..) => "generic class",
        RuntimeTy::Interface(..) => "interface",
        RuntimeTy::Enum(..) | RuntimeTy::EnumVariant(..) => "enum",
        RuntimeTy::List(..) => "list of unsupported elements",
        RuntimeTy::Map { .. } => "map",
        RuntimeTy::Union(members)
            if members
                .iter()
                .any(|member| matches!(member, RuntimeTy::Null)) =>
        {
            "optional scalar or union"
        }
        RuntimeTy::Union(..) => "union",
        RuntimeTy::Function { .. } => "function",
        RuntimeTy::Future(..) => "future",
        RuntimeTy::TypeVar(..) => "type variable",
        RuntimeTy::Never => "never",
        RuntimeTy::Unknown => "unknown",
        _ => "other",
    }
}

fn unsupported_type(ty: &RuntimeTy, detail: impl Into<String>) -> Blocker {
    Blocker::new(format!("{} value", type_kind(ty)), detail)
}

/// The slot each field of a layout starts at: fields take their slots in
/// declaration order.
pub(crate) fn field_slots(layout: &[NativeTy<'_>]) -> Vec<usize> {
    layout
        .iter()
        .scan(0, |next, field| {
            let slot = *next;
            *next += field.slots();
            Some(slot)
        })
        .collect()
}

/// How many slots an instance with this layout has.
pub(crate) fn layout_slots(layout: &[NativeTy<'_>]) -> usize {
    layout.iter().map(NativeTy::slots).sum()
}

/// The types of the fields of a class at these type arguments, in
/// declaration order.
pub(crate) fn class_layout<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class: ClassRef<'db>,
    arguments: &[TypeArg<'db>],
) -> Result<Vec<NativeTy<'db>>, Blocker> {
    let head = class_head(db, class);
    let aliases = resolved_aliases_for_package(db, head.root());
    class_fields(db, class)
        .iter()
        .map(|(name, ty)| {
            let ty = aliases.0.convert(ty);
            native_ty(db, &ty, arguments).ok_or_else(|| {
                Blocker::new(
                    format!(
                        "class field that is a {}",
                        unsupported_type(&ty, "").construct
                    ),
                    name.to_string(),
                )
            })
        })
        .collect()
}

/// Whether the handler that lands with `landing` reads its context: names
/// the local anywhere but as what a rethrow passes on.
pub(crate) fn binds_context(body: &MirFunctionBody<'_>, landing: Landing) -> bool {
    use std::fmt::Write as _;

    let context = format!("{:?}", landing.context_local);
    body.blocks.iter().any(|block| {
        let mut code = String::new();
        for statement in &block.statements {
            let _ = write!(code, "{:?};", statement.kind);
        }
        let _ = match &block.terminator {
            Some(Terminator::Rethrow { value, .. }) => write!(code, "{value:?}"),
            Some(terminator) => write!(code, "{terminator:?}"),
            None => Ok(()),
        };
        code.contains(&context)
    })
}

/// The fields of `baml.spawn.Plan` a `spawn` reads, by position.
pub(crate) const PLAN_BODY: usize = 0;
pub(crate) const PLAN_SEMAPHORES: usize = 2;
pub(crate) const PLAN_CANCEL_TOKENS: usize = 3;

/// What a `spawn` of a plan of this type runs: the signature of the plan's
/// body, when the type is laid out as `baml.spawn.Plan` is.
pub(crate) fn plan_body<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    plan: &NativeTy<'db>,
) -> Option<(Vec<NativeTy<'db>>, Signature<'db>)> {
    let NativeTy::Class(class, arguments) = plan else {
        return None;
    };
    let fields = class_layout(db, *class, arguments).ok()?;
    match (
        fields.get(PLAN_BODY),
        fields.get(PLAN_SEMAPHORES),
        fields.get(PLAN_CANCEL_TOKENS),
    ) {
        (Some(NativeTy::Function(body)), Some(NativeTy::Array(_)), Some(NativeTy::Array(_)))
            if body.params.is_empty() && !body.result.is_pair() =>
        {
            let body = (**body).clone();
            Some((fields, body))
        }
        _ => None,
    }
}

/// A function body together with the types of the locals whose declarations
/// do not give one.
///
/// After the optimizer has replaced an object by its fields, a temporary can
/// be left declared with the type of something it no longer holds, or as
/// `unknown`. Such a local is only ever a copy of other values, so its type
/// is theirs wherever its declaration could not hold them.
pub(crate) struct Body<'a, 'db> {
    /// The function the body belongs to: closures name its lambdas.
    pub(crate) mir: &'a MirFunction<'db>,
    inner: &'a MirFunctionBody<'db>,
    /// What the cells a lambda captures hold, in capture order.
    captures: &'a [NativeTy<'db>],
    /// The copy of the function being typed.
    pub(crate) key: &'a FnKey<'db>,
    /// The type arguments this copy of the function is compiled for.
    pub(crate) arguments: &'a [TypeArg<'db>],
    inferred: Vec<Option<NativeTy<'db>>>,
    /// The locals declared with a type that names a type parameter and are
    /// not parameters. An inlined function's locals keep naming *its* type
    /// parameters, which are not this function's, so such a declaration says
    /// nothing: the local has the type of what is written to it, or none.
    undeclared: Vec<bool>,
    /// The type each local of type `type` is loaded with, when it is loaded
    /// once.
    type_values: Vec<Option<&'a TyTemplate>>,
    /// The locals an array loop keeps as an array and a position; see
    /// [`crate::loops`].
    loop_locals: Vec<bool>,
    /// The locals a handler lands with: the error and its context. A
    /// handler catches whatever is thrown, so the error is held as an
    /// `unknown` whatever type the local is declared with, and tested
    /// before it is used as anything narrower.
    landed: Vec<bool>,
    /// For each block, the member each local of a union type is known to
    /// hold throughout it; see [`crate::narrow`].
    narrowed: Vec<Vec<Option<NativeTy<'db>>>>,
    /// Whether any run reaches each block.
    reachable: Vec<bool>,
    /// The block whose code is being typed, if any is.
    at: Cell<Option<usize>>,
}

impl<'db> std::ops::Deref for Body<'_, 'db> {
    type Target = MirFunctionBody<'db>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a, 'db> Body<'a, 'db> {
    pub(crate) fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        mir: &'a MirFunction<'db>,
        body: &'a MirFunctionBody<'db>,
        captures: &'a [NativeTy<'db>],
        key: &'a FnKey<'db>,
    ) -> Self {
        let arguments = &key.arguments[..];
        let type_values = loaded_types(body);
        let mut typed = Self {
            mir,
            inner: body,
            captures,
            key,
            arguments,
            undeclared: body
                .locals
                .iter()
                .enumerate()
                .map(|(index, local)| index > mir.arity && names_type_parameter(&local.ty))
                .collect(),
            type_values,
            inferred: vec![None; body.locals.len()],
            loop_locals: vec![false; body.locals.len()],
            landed: {
                let mut landed = vec![false; body.locals.len()];
                for (_, landing) in body.handlers() {
                    landed[landing.error_local.0] = true;
                    // A context that is read is the `baml.errors.Context` it
                    // is declared as; one that is only passed on is the
                    // trace of the throw, or nothing.
                    landed[landing.context_local.0] = !binds_context(body, landing);
                }
                landed
            },
            narrowed: Vec::new(),
            reachable: vec![true; body.blocks.len()],
            at: Cell::new(None),
        };
        // A local written any other way than by copying has the type it is
        // declared with.
        let mut copied_only = vec![true; body.locals.len()];
        // ... or, for a local declared with a type that has no
        // representation (a type parameter of a function inlined here), by
        // copying and by operators, whose operands say what they make.
        let mut computed_only = vec![true; body.locals.len()];
        for local in 0..body.locals.len().min(mir.arity + 1) {
            copied_only[local] = false;
            computed_only[local] = false;
        }
        for block in &body.blocks {
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value,
                    } if !matches!(value, Rvalue::Use(_)) => {
                        copied_only[local.0] = false;
                        if !matches!(value, Rvalue::BinaryOp { .. } | Rvalue::UnaryOp { .. }) {
                            computed_only[local.0] = false;
                        }
                    }
                    StatementKind::FreshCell { local, .. } => {
                        copied_only[local.0] = false;
                        computed_only[local.0] = false;
                    }
                    _ => {}
                }
            }
            if let Some(local) = block.terminator.as_ref().and_then(terminator_destination) {
                copied_only[local.0] = false;
                computed_only[local.0] = false;
            }
        }
        // A local assigned values of two different types has no type.
        let mut conflicting = vec![false; body.locals.len()];
        loop {
            let mut changed = false;
            let assignments = body
                .blocks
                .iter()
                .flat_map(|block| &block.statements)
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value,
                    } => Some((*local, Written::Value(value))),
                    _ => None,
                });
            let results = body.blocks.iter().filter_map(|block| {
                let terminator = block.terminator.as_ref()?;
                Some((
                    terminator_destination(terminator)?,
                    Written::Result(terminator),
                ))
            });
            for (local, written) in assignments.chain(results) {
                let declaration = body.local(local);
                let undeclared = typed.undeclared[local.0];
                let declared = if undeclared {
                    None
                } else {
                    native_ty(db, &declaration.ty, arguments)
                };
                // The temporary an `await spawn { .. }` keeps the future in
                // is declared `null`: it has the type of what is spawned.
                let spawned = matches!(written, Written::Result(Terminator::Spawn { .. }));
                let inferable = undeclared
                    || spawned
                    || copied_only[local.0]
                    || (declared.is_none() && computed_only[local.0]);
                if declaration.is_captured || conflicting[local.0] || !inferable {
                    continue;
                }
                let found = match written {
                    Written::Value(Rvalue::Use(operand)) => operand_ty(db, &typed, operand),
                    Written::Value(Rvalue::BinaryOp { op, left, right }) => {
                        operand_ty(db, &typed, left)
                            .and_then(|left| Ok((left, operand_ty(db, &typed, right)?)))
                            .and_then(|(left, right)| binary_typing(*op, &left, &right))
                            .map(|(_, result)| result)
                    }
                    Written::Value(Rvalue::UnaryOp { op, operand }) => {
                        operand_ty(db, &typed, operand)
                            .and_then(|operand| unary_typing(*op, &operand))
                    }
                    Written::Result(Terminator::Spawn { plan, .. }) => {
                        match operand_ty(db, &typed, plan)
                            .ok()
                            .and_then(|plan| plan_body(db, &narrowed(plan)))
                        {
                            Some((_, body)) => Ok(NativeTy::Future(Box::new(body.result))),
                            None => continue,
                        }
                    }
                    // What is left is only ever the one write of a local an
                    // inlined function declared.
                    _ if !undeclared => continue,
                    Written::Value(Rvalue::Aggregate {
                        kind:
                            AggregateKind::Class {
                                class,
                                type_arg_templates,
                                ..
                            },
                        ..
                    }) => Ok(NativeTy::Class(
                        *class,
                        type_arg_templates
                            .iter()
                            .map(|template| template_ty(db, template, arguments))
                            .collect(),
                    )),
                    Written::Value(Rvalue::Array(element, _)) => {
                        match template_ty(db, element, arguments) {
                            Some(element) if !element.is_pair() => {
                                Ok(NativeTy::Array(Box::new(element)))
                            }
                            _ => continue,
                        }
                    }
                    Written::Result(Terminator::Call {
                        callee,
                        args,
                        ntypeargs,
                        ..
                    }) => match call_target(db, &typed, callee, args, *ntypeargs) {
                        Ok(Callee::Function(key)) => {
                            function_signature(db, &key).map(|signature| signature.result)
                        }
                        Ok(Callee::Value) => match operand_ty(db, &typed, callee).map(narrowed) {
                            Ok(NativeTy::Function(signature)) => Ok(signature.result),
                            _ => continue,
                        },
                        Ok(Callee::Native(native)) => {
                            native.callable().and_then(|(signature, _)| {
                                crate::check::native_type_arguments(
                                    db, &typed, native, args, *ntypeargs,
                                )
                                .map(|types| signature.result.native(&types))
                            })
                        }
                        Ok(Callee::Intrinsic(intrinsic)) => args[*ntypeargs..]
                            .iter()
                            .map(|argument| operand_ty(db, &typed, argument))
                            .collect::<Result<Vec<_>, _>>()
                            .and_then(|found| {
                                intrinsic.result(&found).ok_or_else(|| {
                                    Blocker::new("builtin call on these argument types", "")
                                })
                            }),
                        _ => continue,
                    },
                    // An operator method on primitives has the operator's
                    // result.
                    Written::Result(Terminator::VirtualCall {
                        iface,
                        method,
                        args,
                        ntypeargs,
                        self_arg,
                        ..
                    }) => match crate::check::virtual_target(
                        db, &typed, iface, method, args, *ntypeargs, *self_arg,
                    ) {
                        Ok(crate::check::Virtual::Operator(operator)) => args
                            .iter()
                            .map(|argument| operand_ty(db, &typed, argument))
                            .collect::<Result<Vec<_>, _>>()
                            .and_then(|operands| operator.result(&operands)),
                        _ => continue,
                    },
                    _ => continue,
                };
                let Ok(found) = found else {
                    continue;
                };
                // A declaration that can hold what is copied stands. One
                // that says `unknown` would only put the value in a box.
                if declared.is_some_and(|declared| {
                    declared != NativeTy::Unknown && assignable(&declared, &found)
                }) {
                    continue;
                }
                match &typed.inferred[local.0] {
                    None => {
                        typed.inferred[local.0] = Some(found);
                        changed = true;
                    }
                    Some(known) if *known != found => {
                        typed.inferred[local.0] = None;
                        conflicting[local.0] = true;
                        changed = true;
                    }
                    Some(_) => {}
                }
            }
            if !changed {
                break;
            }
        }
        let loops = ArrayLoops::of(db, &typed);
        for (local, is_loop_local) in typed.loop_locals.iter_mut().enumerate() {
            *is_loop_local =
                loops.iterator(Local(local)).is_some() || loops.item(Local(local)).is_some();
        }
        let found = crate::narrow::block_types(db, &typed);
        typed.narrowed = found.narrowed;
        typed.reachable = found.reachable;
        typed
    }

    /// Whether any run reaches `block`. One that none does is not compiled.
    pub(crate) fn is_reachable(&self, block: usize) -> bool {
        self.reachable[block]
    }

    /// The type a local of type `type` holds, when it is loaded once.
    pub(crate) fn type_value(&self, operand: &Operand<'db>) -> Option<&'a TyTemplate> {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                self.type_values[local.0]
            }
            _ => None,
        }
    }

    /// Type what follows as the code of `block`.
    /// The block whose code is being typed, if any is.
    pub(crate) fn current(&self) -> Option<usize> {
        self.at.get()
    }

    pub(crate) fn enter(&self, block: usize) {
        self.at.set(Some(block));
    }
}

/// The type each local of type `type` is loaded with, when it is loaded
/// once and written no other way.
pub(crate) fn loaded_types<'a>(body: &'a MirFunctionBody<'_>) -> Vec<Option<&'a TyTemplate>> {
    let mut types: Vec<Option<&'a TyTemplate>> = vec![None; body.locals.len()];
    let mut writes = vec![0_usize; body.locals.len()];
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                value,
            } = &statement.kind
            {
                writes[local.0] += 1;
                if let Rvalue::LoadType(template) = value {
                    types[local.0] = Some(template);
                }
            }
        }
        if let Some(local) = block.terminator.as_ref().and_then(terminator_destination) {
            writes[local.0] += 1;
        }
    }
    for (loaded, writes) in types.iter_mut().zip(writes) {
        if writes != 1 {
            *loaded = None;
        }
    }
    types
}

/// One write of a local: an assignment's value, or what a terminator
/// leaves in its destination.
#[derive(Clone, Copy)]
enum Written<'a, 'db> {
    Value(&'a Rvalue<'db>),
    Result(&'a Terminator<'db>),
}

/// The type a local is declared with, when it is a native one. A local
/// that a closure captures is a cell of its declared type.
pub(crate) fn declared_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &MirFunctionBody<'_>,
    local: Local,
    arguments: &[TypeArg<'db>],
) -> Result<NativeTy<'db>, Blocker> {
    let declaration = body.local(local);
    let ty = native_ty(db, &declaration.ty, arguments)
        .ok_or_else(|| unsupported_type(&declaration.ty, local.to_string()))?;
    Ok(if declaration.is_captured {
        NativeTy::Cell(Box::new(ty))
    } else {
        ty
    })
}

/// What the cell a place names holds.
fn cell_content<'db>(ty: NativeTy<'db>, place: &Place) -> Result<NativeTy<'db>, Blocker> {
    match ty {
        NativeTy::Cell(content) => Ok(*content),
        _ => Err(Blocker::new(
            "closure cell access outside a cell",
            place.to_string(),
        )),
    }
}

/// The type of the value a local holds in the block being typed: for a local
/// of a union type, the member it is known to hold there, if one is.
pub(crate) fn local_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    local: Local,
) -> Result<NativeTy<'db>, Blocker> {
    if let Some(block) = body.at.get()
        && let Some(Some(narrowed)) = body
            .narrowed
            .get(block)
            .map(|locals| locals[local.0].as_ref())
    {
        return Ok(narrowed.clone());
    }
    storage_ty(db, body, local)
}

/// The type of the variable a local is kept in.
pub(crate) fn storage_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    local: Local,
) -> Result<NativeTy<'db>, Blocker> {
    if body.loop_locals[local.0] {
        return Err(Blocker::new(
            "array loop local used outside its loop",
            local.to_string(),
        ));
    }
    if body.landed[local.0] {
        return Ok(NativeTy::Unknown);
    }
    match &body.inferred[local.0] {
        Some(inferred) if !body.local(local).is_captured => Ok(inferred.clone()),
        _ if body.undeclared[local.0] => Err(Blocker::new(
            "type variable value",
            format!("{local} has no one type"),
        )),
        _ => declared_ty(db, body, local, body.arguments),
    }
}

/// The local a place names, when it is a plain local.
pub(crate) fn place_local(place: &Place) -> Result<Local, Blocker> {
    match place {
        Place::Local(local) => Ok(*local),
        other => Err(Blocker::new(
            "call result stored outside a local",
            other.to_string(),
        )),
    }
}

/// The reference type behind an optional. MIR projects through an optional
/// only where the checker proved it is not null.
pub(crate) fn narrowed(ty: NativeTy<'_>) -> NativeTy<'_> {
    match ty {
        NativeTy::Optional(inner) => *inner,
        other => other,
    }
}

/// The type of the value a place holds.
pub(crate) fn place_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    place: &Place,
) -> Result<NativeTy<'db>, Blocker> {
    match place {
        Place::Local(local) => local_ty(db, body, *local),
        Place::Field { base, field } => match narrowed(place_ty(db, body, base)?) {
            NativeTy::Class(class, arguments) => class_layout(db, class, &arguments)?
                .into_iter()
                .nth(*field)
                .ok_or_else(|| Blocker::new("field outside its class layout", place.to_string())),
            _ => Err(Blocker::new(
                "field access on a non-class value",
                place.to_string(),
            )),
        },
        Place::Index {
            base,
            index,
            kind: IndexKind::Array,
        } => {
            if local_ty(db, body, *index)? != NativeTy::Int {
                return Err(Blocker::new(
                    "array index that is not an int",
                    place.to_string(),
                ));
            }
            match narrowed(place_ty(db, body, base)?) {
                NativeTy::Array(element) => Ok(*element),
                _ => Err(Blocker::new(
                    "index access on a non-array value",
                    place.to_string(),
                )),
            }
        }
        Place::Index {
            base,
            index,
            kind: IndexKind::Map,
        } => match narrowed(place_ty(db, body, base)?) {
            NativeTy::Map(key, value) if local_ty(db, body, *index)? == *key => Ok(*value),
            NativeTy::Map(..) => Err(Blocker::new(
                "map index that is not of the key type",
                place.to_string(),
            )),
            _ => Err(Blocker::new(
                "index access on a non-map value",
                place.to_string(),
            )),
        },
        Place::Index {
            base,
            index,
            kind: IndexKind::Tuple,
        } => match narrowed(place_ty(db, body, base)?) {
            NativeTy::Tuple(elements) => tuple_element(body, *index)
                .and_then(|element| elements.into_iter().nth(element))
                .ok_or_else(|| {
                    Blocker::new("tuple index that is not a constant", place.to_string())
                }),
            _ => Err(Blocker::new(
                "tuple index access on a non-tuple value",
                place.to_string(),
            )),
        },
        Place::Capture(index) => body
            .captures
            .get(*index)
            .map(|content| NativeTy::Cell(Box::new(content.clone())))
            .ok_or_else(|| Blocker::new("capture outside its closure", place.to_string())),
        Place::Deref(CellId::Local(local)) => cell_content(local_ty(db, body, *local)?, place),
        Place::Deref(CellId::Capture(index)) => {
            cell_content(place_ty(db, body, &Place::Capture(*index))?, place)
        }
    }
}

/// Which element of a tuple the local `index` names: the local is declared
/// with the literal type of the one value it holds, or is written once,
/// with a constant.
pub(crate) fn tuple_element(body: &MirFunctionBody<'_>, index: Local) -> Option<usize> {
    if let RuntimeTy::Literal(Literal::Int(element), _) = &body.local(index).ty {
        return usize::try_from(*element).ok();
    }
    let mut element = None;
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                value,
            } = &statement.kind
                && *local == index
            {
                match value {
                    Rvalue::Use(Operand::Constant(Constant::Int(value))) if element.is_none() => {
                        element = Some(usize::try_from(*value).ok()?);
                    }
                    _ => return None,
                }
            }
        }
        if block.terminator.as_ref().and_then(terminator_destination) == Some(index) {
            return None;
        }
    }
    element
}

pub(crate) fn constant_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    constant: &Constant<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    match constant {
        Constant::Int(_) => Ok(NativeTy::Int),
        Constant::Float(_) => Ok(NativeTy::Float),
        Constant::Bool(_) => Ok(NativeTy::Bool),
        Constant::Null => Ok(NativeTy::Null),
        Constant::String(_) => Ok(NativeTy::String),
        Constant::Bigint(_) => Err(Blocker::new("bigint value", "constant")),
        Constant::OmittedArg => Err(Blocker::new("defaulted argument", "")),
        Constant::Function(DeclRef::Source(function)) => Ok(NativeTy::Function(Box::new(
            function_signature(db, &FnKey::new(*function, Entry::Direct))?,
        ))),
        Constant::Function(DeclRef::External(_)) => {
            Err(Blocker::new("function value", "a function without source"))
        }
        Constant::GenericFunction { .. } => Err(Blocker::new("function value", "generic")),
        Constant::GlobalItem(_) => Err(Blocker::new("global read", "")),
        Constant::EnumVariant { enum_ref, .. } => Ok(NativeTy::Enum(*enum_ref)),
    }
}

pub(crate) fn operand_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    operand: &Operand<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_ty(db, body, place),
        Operand::Constant(constant) => constant_ty(db, constant),
    }
}

/// Whether `operand` stands for an argument a call leaves to its default.
pub(crate) fn is_omitted(operand: &Operand<'_>) -> bool {
    matches!(operand, Operand::Constant(Constant::OmittedArg))
}

/// Whether a parameter of type `ty` can be passed as left to its default:
/// there is a word no value of the type is. An enum has a position no
/// variant is at, a `bool` a byte that is neither, and a reference an
/// address no object is at. Every word is an `int` or a `float`.
pub(crate) fn has_omitted_form(ty: &NativeTy<'_>) -> bool {
    match ty {
        NativeTy::Enum(_) | NativeTy::Bool => true,
        NativeTy::Cell(_) => false,
        other => other.is_ref(),
    }
}

/// Whether a value of type `found` can be stored where `expected` is
/// declared: the same type, a reference and its optional either way round,
/// or `null` where an optional is expected.
pub(crate) fn assignable(expected: &NativeTy<'_>, found: &NativeTy<'_>) -> bool {
    expected == found
        || match (expected, found) {
            // A union takes whatever the type checker let through, and
            // gives it back where flow analysis proved what it holds.
            (expected, found) if expected.is_boxed() || found.is_boxed() => {
                !matches!(expected, NativeTy::Cell(_)) && !matches!(found, NativeTy::Cell(_))
            }
            (NativeTy::Optional(inner), found) => {
                *found == NativeTy::Null || assignable(inner, found)
            }
            // Flow narrowing: MIR reads an optional as its reference where
            // the checker proved it is not null.
            (expected, NativeTy::Optional(inner)) => assignable(expected, inner),
            // The same slots, holding what the type checker let through.
            (NativeTy::Tuple(expected), NativeTy::Tuple(found)) => {
                expected.len() == found.len()
                    && expected.iter().zip(found).all(|(expected, found)| {
                        expected == found
                            || (expected.is_ref() && found.is_ref() && assignable(expected, found))
                    })
            }
            // A function compiled once for every reference type takes and
            // gives an instance or an array at any of them.
            (NativeTy::Class(expected, wanted), NativeTy::Class(found, given)) => {
                expected == found
                    && wanted.len() == given.len()
                    && wanted.iter().zip(given).all(|(wanted, given)| {
                        type_argument(wanted.clone()) == type_argument(given.clone())
                    })
            }
            (NativeTy::Array(expected), NativeTy::Array(found)) => {
                expected.is_ref() && found.is_ref()
            }
            (NativeTy::Map(expected_key, expected), NativeTy::Map(found_key, found)) => {
                expected_key == found_key && expected.is_ref() && found.is_ref()
            }
            // As for a class at its type arguments: a future's slot holds
            // any reference the same way.
            (NativeTy::Future(expected), NativeTy::Future(found)) => {
                type_argument(Some((**expected).clone())) == type_argument(Some((**found).clone()))
            }
            // The type checker has decided the two function types agree;
            // what is left to ask is that a call passes the same registers.
            (NativeTy::Function(expected), NativeTy::Function(found)) => {
                expected.params.len() == found.params.len()
                    && expected.result.repr() == found.result.repr()
                    && expected
                        .params
                        .iter()
                        .zip(&found.params)
                        .all(|(expected, found)| expected.repr() == found.repr())
            }
            _ => false,
        }
}

/// How a binary operator is evaluated for a pair of operand types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BinaryMode {
    /// Both operands are ints.
    Int,
    /// At least one operand is a float; an int operand is converted first.
    Float,
    /// Both operands are bools; only `==` and `!=`.
    Bool,
    /// Both operands are null; only `==` and `!=`.
    Null,
    /// Both operands are strings: `+` and the comparisons.
    String,
    /// An optional against `null`; only `==` and `!=`. The flag says the
    /// optional is the left operand.
    NullTest { optional_on_left: bool },
    /// Two maps of string keys, compared key by key in whatever order.
    Map,
}

pub(crate) fn is_comparison(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

/// The evaluation mode and result type of `left op right`.
pub(crate) fn binary_typing<'db>(
    op: BinOp,
    left: &NativeTy<'db>,
    right: &NativeTy<'db>,
) -> Result<(BinaryMode, NativeTy<'db>), Blocker> {
    let unsupported = || {
        Blocker::new(
            "binary operator on these operand types",
            format!("{} {op} {}", describe(left), describe(right)),
        )
    };
    let equality = matches!(op, BinOp::Eq | BinOp::Ne);
    let mode = match (left, right) {
        (NativeTy::Int, NativeTy::Int) => BinaryMode::Int,
        (NativeTy::Float, NativeTy::Float | NativeTy::Int) | (NativeTy::Int, NativeTy::Float) => {
            if matches!(
                op,
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
            ) {
                return Err(unsupported());
            }
            BinaryMode::Float
        }
        (NativeTy::Bool, NativeTy::Bool) if equality => BinaryMode::Bool,
        (NativeTy::Null, NativeTy::Null) if equality => BinaryMode::Null,
        (NativeTy::String, NativeTy::String) if is_comparison(op) || op == BinOp::Add => {
            BinaryMode::String
        }
        // The runtime compares the values, and calls no `equals`.
        (NativeTy::Map(key, value), NativeTy::Map(other_key, other_value))
            if equality
                && **key == NativeTy::String
                && key == other_key
                && value == other_value
                && is_map_key(value) =>
        {
            BinaryMode::Map
        }
        (NativeTy::Optional(_) | NativeTy::Union(_) | NativeTy::Unknown, NativeTy::Null)
            if equality =>
        {
            BinaryMode::NullTest {
                optional_on_left: true,
            }
        }
        (NativeTy::Null, NativeTy::Optional(_) | NativeTy::Union(_) | NativeTy::Unknown)
            if equality =>
        {
            BinaryMode::NullTest {
                optional_on_left: false,
            }
        }
        _ => return Err(unsupported()),
    };
    let result = if is_comparison(op) {
        NativeTy::Bool
    } else {
        match mode {
            BinaryMode::Int => NativeTy::Int,
            BinaryMode::Float => NativeTy::Float,
            BinaryMode::String => NativeTy::String,
            BinaryMode::Bool | BinaryMode::Null | BinaryMode::NullTest { .. } | BinaryMode::Map => {
                return Err(unsupported());
            }
        }
    };
    Ok((mode, result))
}

/// A short name for a type in a blocker's detail.
pub(crate) fn describe(ty: &NativeTy<'_>) -> &'static str {
    match ty {
        NativeTy::Int => "int",
        NativeTy::Float => "float",
        NativeTy::Bool => "bool",
        NativeTy::Null => "null",
        NativeTy::Enum(_) => "enum",
        NativeTy::Union(_) => "union",
        NativeTy::Unknown => "unknown",
        NativeTy::String => "string",
        NativeTy::Class(..) => "class",
        NativeTy::Tuple(_) => "tuple",
        NativeTy::Array(_) => "array",
        NativeTy::Map(..) => "map",
        NativeTy::Function(_) => "function",
        NativeTy::Cell(_) => "cell",
        NativeTy::Optional(_) => "optional",
        NativeTy::Future(_) => "future",
    }
}

/// The result type of `op operand`.
pub(crate) fn unary_typing<'db>(
    op: UnaryOp,
    operand: &NativeTy<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    let unsupported = || {
        Blocker::new(
            "unary operator on this operand type",
            format!("{op}{}", describe(operand)),
        )
    };
    match (op, operand) {
        (
            UnaryOp::Not | UnaryOp::Truthy,
            NativeTy::Int | NativeTy::Float | NativeTy::Bool | NativeTy::Null,
        ) => Ok(NativeTy::Bool),
        (UnaryOp::Neg, NativeTy::Int) => Ok(NativeTy::Int),
        (UnaryOp::Neg, NativeTy::Float) => Ok(NativeTy::Float),
        _ => Err(unsupported()),
    }
}
