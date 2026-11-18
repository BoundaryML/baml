//! Decides whether a MIR function is in the subset native code is generated
//! for, and names every construct that keeps it out.

use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_mir::{
    AggregateKind, BinOp, BoundFrame, Constant, Local, MirFunction, MirFunctionBody,
    MirFunctionKind, Operand, OptLevel, Place, RuntimeTy, Rvalue, ShortCircuitKind, StatementKind,
    SwitchKey, Terminator, TyTemplate, TyTemplateInterface, TypeTest, UnaryOp,
    function_display_name,
};

use crate::{
    Blocker,
    dispatch::{self, Slot, Target},
    loops::{ArrayLoops, is_done_test, operand_local},
    narrow::TestKind,
    program::{Entry, FnKey, Needed},
    types::{
        Body, NativeTy, Signature, TypeArg, assignable, binary_typing, class_layout, declared_ty,
        describe, loaded_types, local_ty, narrowed, operand_ty, place_local, place_ty, template_ty,
        type_argument, unary_typing,
    },
};

/// The optimization level every function of a native program is lowered at.
pub(crate) const OPT: OptLevel = OptLevel::Two;

/// A standard library function implemented by the interpreter that native
/// code implements itself, inline or in its runtime.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Intrinsic {
    /// `array.push(item)`, returning the new length.
    ArrayPush,
    /// The number of characters in a string.
    StringCharCount,
    /// The text of an `int`, `float`, `bool`, `null` or `string`: what a
    /// template hole and `to_string()` produce for them.
    ToString,
    /// `==` and `!=` between two variants of an enum, or two maps of
    /// string keys whose values the runtime compares.
    EnumEq,
    EnumNe,
    /// A method of `float` or `int` that is one instruction.
    Scalar(ScalarOp),
}

/// The `float` and `int` methods compiled to one instruction each.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ScalarOp {
    FloatSqrt,
    FloatAbs,
    FloatFloor,
    FloatCeil,
    FloatTrunc,
    FloatIsNan,
    IntAbs,
    IntLeadingZeros,
    IntTrailingZeros,
    IntCountOnes,
}

impl Intrinsic {
    fn named(name: &str) -> Option<Self> {
        match name {
            "baml.Array.push" => Some(Self::ArrayPush),
            // `length` is declared as `char_count`.
            "baml.String.char_count" | "baml.String.length" => Some(Self::StringCharCount),
            "baml._to_string_default" | "baml._to_string_primitive" => Some(Self::ToString),
            "baml.ops.equals_equals" => Some(Self::EnumEq),
            "baml.ops.not_equals" => Some(Self::EnumNe),
            "baml.Float.sqrt" => Some(Self::Scalar(ScalarOp::FloatSqrt)),
            "baml.Float.abs" => Some(Self::Scalar(ScalarOp::FloatAbs)),
            "baml.Float.floor" => Some(Self::Scalar(ScalarOp::FloatFloor)),
            "baml.Float.ceil" => Some(Self::Scalar(ScalarOp::FloatCeil)),
            "baml.Float.trunc" => Some(Self::Scalar(ScalarOp::FloatTrunc)),
            "baml.Float.is_nan" => Some(Self::Scalar(ScalarOp::FloatIsNan)),
            "baml.Int.abs" => Some(Self::Scalar(ScalarOp::IntAbs)),
            "baml.Int.leading_zeros" => Some(Self::Scalar(ScalarOp::IntLeadingZeros)),
            "baml.Int.trailing_zeros" => Some(Self::Scalar(ScalarOp::IntTrailingZeros)),
            "baml.Int.count_ones" => Some(Self::Scalar(ScalarOp::IntCountOnes)),
            _ => None,
        }
    }

    /// The result type for value arguments of these types, when they fit.
    pub(crate) fn result<'db>(self, arguments: &[NativeTy<'db>]) -> Option<NativeTy<'db>> {
        match (self, arguments) {
            (Self::ArrayPush, [NativeTy::Array(element), item]) if assignable(element, item) => {
                Some(NativeTy::Int)
            }
            (Self::StringCharCount, [NativeTy::String]) => Some(NativeTy::Int),
            (Self::Scalar(ScalarOp::FloatIsNan), [NativeTy::Float]) => Some(NativeTy::Bool),
            (
                Self::Scalar(
                    ScalarOp::FloatSqrt
                    | ScalarOp::FloatAbs
                    | ScalarOp::FloatFloor
                    | ScalarOp::FloatCeil
                    | ScalarOp::FloatTrunc,
                ),
                [NativeTy::Float],
            ) => Some(NativeTy::Float),
            (
                Self::Scalar(
                    ScalarOp::IntAbs
                    | ScalarOp::IntLeadingZeros
                    | ScalarOp::IntTrailingZeros
                    | ScalarOp::IntCountOnes,
                ),
                [NativeTy::Int],
            ) => Some(NativeTy::Int),
            (Self::EnumEq | Self::EnumNe, [NativeTy::Enum(left), NativeTy::Enum(right)])
                if left == right =>
            {
                Some(NativeTy::Bool)
            }
            // Two maps the runtime compares itself.
            (
                Self::EnumEq | Self::EnumNe,
                [left @ NativeTy::Map(..), right @ NativeTy::Map(..)],
            ) if matches!(
                crate::types::binary_typing(BinOp::Eq, left, right),
                Ok((crate::types::BinaryMode::Map, _))
            ) =>
            {
                Some(NativeTy::Bool)
            }
            (
                Self::ToString,
                [
                    NativeTy::Int
                    | NativeTy::Float
                    | NativeTy::Bool
                    | NativeTy::Null
                    | NativeTy::Enum(_)
                    | NativeTy::String
                    | NativeTy::Bigint,
                ],
            ) => Some(NativeTy::String),
            _ => None,
        }
    }
}

/// What a call terminator calls.
pub(crate) enum Callee<'db> {
    /// The copy of a function compiled for the call's type arguments.
    Function(FnKey<'db>),
    Intrinsic(Intrinsic),
    /// A standard library builtin the native runtime implements in Rust.
    Native(&'static crate::natives::Native),
    /// The function value an operand holds.
    Value,
}

/// The body of a function that has one in MIR.
pub(crate) fn body_of<'a, 'db>(
    mir: &'a MirFunction<'db>,
) -> Result<&'a MirFunctionBody<'db>, Blocker> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => Ok(body),
        MirFunctionKind::Builtin(kind) => Err(Blocker::new(
            "runtime builtin",
            format!("{kind:?}").to_lowercase(),
        )),
    }
}

/// Local `_0` is the result and `_1..=arity` are the parameters.
pub(crate) fn signature<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    mir: &MirFunction<'_>,
    body: &MirFunctionBody<'_>,
    arguments: &[TypeArg<'db>],
) -> Result<Signature<'db>, Blocker> {
    let rename = |blocker: Blocker, what: &str| {
        Blocker::new(format!("{what} is a {}", blocker.construct), blocker.detail)
    };
    let result =
        declared_ty(db, body, Local(0), arguments).map_err(|blocker| rename(blocker, "result"))?;
    let params = (1..=mir.arity)
        .map(
            |index| match declared_ty(db, body, Local(index), arguments) {
                // A captured parameter arrives as its value; the function puts
                // it in a cell.
                Ok(NativeTy::Cell(content)) => Ok(*content),
                Ok(ty) => Ok(ty),
                Err(blocker) => Err(rename(blocker, "parameter")),
            },
        )
        .collect::<Result<_, _>>()?;
    Ok(Signature { params, result })
}

/// The signature of one copy of a function.
pub(crate) fn function_signature<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    key: &FnKey<'db>,
) -> Result<Signature<'db>, Blocker> {
    let mir = key.mir(db)?;
    signature(db, mir, body_of(mir)?, &key.arguments)
}

/// Which of its type arguments a copy of a function must be told the type
/// id of when it runs: those it was compiled for as "any reference" and
/// tests a value against, itself or in a function it calls. A copy with
/// any takes one type id for every type parameter of its frame.
pub(crate) fn identified_arguments<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    key: &FnKey<'db>,
) -> Vec<bool> {
    identified(db, key, &mut Vec::new(), &mut Vec::new())
        .iter()
        .map(|uses| *uses != 0)
        .collect()
}

/// A value is tested against the type argument: it must have one type id.
const TESTED: u8 = 1;
/// The type argument is handed on to `baml.json`, which takes the
/// description of any type it can write and read.
const DESCRIBED: u8 = 2;

fn identified<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    key: &FnKey<'db>,
    visiting: &mut Vec<FnKey<'db>>,
    assumed: &mut Vec<(FnKey<'db>, Vec<u8>)>,
) -> Vec<u8> {
    let any_reference = |slot: usize| key.arguments.get(slot) == Some(&Some(NativeTy::Unknown));
    let mut needed = vec![0; key.arguments.len()];
    if !(0..key.arguments.len()).any(any_reference) {
        return needed;
    }
    // A function that calls itself needs what it needs: start from nothing
    // and go round until the answer stops growing.
    if visiting.contains(key) {
        return assumed
            .iter()
            .find(|(assumed, _)| assumed == key)
            .map_or(needed, |(_, needed)| needed.clone());
    }
    let Ok(mir) = key.mir(db) else {
        return needed;
    };
    let Ok(body) = body_of(mir) else {
        return needed;
    };
    let types = loaded_types(body);
    visiting.push(key.clone());
    loop {
        let before = needed.clone();
        let mut test = |test: &TypeTest<'db>| {
            if let TypeTest::Template(TyTemplate::TypeArgRef(slot)) = test
                && any_reference(*slot as usize)
            {
                needed[*slot as usize] |= TESTED;
            }
        };
        let mut calls = Vec::new();
        // The type parameters handed on to `baml.json`.
        let mut handed_on = Vec::new();
        for block in &body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    value: Rvalue::IsType { test: asked, .. },
                    ..
                } = &statement.kind
                {
                    test(asked);
                }
            }
            match &block.terminator {
                Some(Terminator::NarrowBind { test: asked, .. }) => test(asked),
                Some(Terminator::Call {
                    callee: Operand::Constant(Constant::Function(function)),
                    args,
                    ntypeargs,
                    ..
                }) if *ntypeargs != 0 => {
                    let templates: Option<Vec<&TyTemplate>> = args[..*ntypeargs]
                        .iter()
                        .map(|argument| operand_local(argument).and_then(|local| types[local.0]))
                        .collect();
                    let native = crate::natives::named(&function_display_name(db, *function));
                    match (templates, native, function) {
                        // `baml.json` is passed the description of each of
                        // its type arguments.
                        (Some(templates), Some(native), _) => {
                            if crate::json::takes_descriptions(native) {
                                for template in templates {
                                    if let TyTemplate::TypeArgRef(slot) = template
                                        && any_reference(*slot as usize)
                                    {
                                        handed_on.push(*slot as usize);
                                    }
                                }
                            }
                        }
                        (Some(templates), None, DeclRef::Source(function)) => {
                            calls.push((*function, templates));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        for slot in handed_on {
            needed[slot] |= DESCRIBED;
        }
        for (function, templates) in calls {
            let callee = FnKey::new(function, Entry::Direct).with_arguments(
                templates
                    .iter()
                    .map(|template| type_argument(template_ty(db, template, &key.arguments)))
                    .collect(),
            );
            let wanted = identified(db, &callee, visiting, assumed);
            for (template, wanted) in templates.iter().zip(wanted) {
                if wanted != 0
                    && let TyTemplate::TypeArgRef(slot) = template
                    && any_reference(*slot as usize)
                {
                    needed[*slot as usize] |= wanted;
                }
            }
        }
        if needed == before {
            break;
        }
        assumed.retain(|(assumed, _)| assumed != key);
        assumed.push((key.clone(), needed.clone()));
    }
    visiting.pop();
    needed
}

/// The one type id of the values of a type, when they have one: what a
/// function is passed for a type argument it must identify.
pub(crate) fn has_type_id(ty: &NativeTy<'_>) -> bool {
    matches!(
        ty,
        NativeTy::Int
            | NativeTy::Float
            | NativeTy::Bool
            | NativeTy::String
            | NativeTy::Enum(_)
            | NativeTy::Class(..)
    )
}

/// The signature of the function value `function` bound to a receiver is:
/// the function's, less its first parameter, which is returned beside it.
fn bound_signature<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'db>,
) -> Result<(NativeTy<'db>, Signature<'db>), Blocker> {
    let mut signature = function_signature(db, &FnKey::new(function, Entry::Direct))?;
    if signature.params.is_empty() {
        return Err(Blocker::new("bound method value", "without a receiver"));
    }
    let receiver = signature.params.remove(0);
    Ok((receiver, signature))
}

/// Whether a call is of a builtin native code has its own implementation
/// of.
pub(crate) fn is_intrinsic_call<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    callee: &Operand<'db>,
) -> bool {
    matches!(callee, Operand::Constant(Constant::Function(function))
        if Intrinsic::named(&function_display_name(db, *function)).is_some())
}

/// What a call names directly: a function written in BAML source, compiled
/// for the call's type arguments (its first `ntypeargs` arguments), or a
/// builtin native code has its own implementation of.
pub(crate) fn call_target<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    callee: &Operand<'db>,
    args: &[Operand<'db>],
    ntypeargs: usize,
) -> Result<Callee<'db>, Blocker> {
    match callee {
        Operand::Constant(Constant::Function(function)) => {
            let name = function_display_name(db, *function);
            if let Some(intrinsic) = Intrinsic::named(&name) {
                return Ok(Callee::Intrinsic(intrinsic));
            }
            // A method written in Rust that takes a closure has a loop form
            // or a driver written in BAML, which is what native code runs.
            let function = match function {
                DeclRef::Source(function) => baml_compiler2_mir::loop_form(db, *function)
                    .or_else(|| baml_compiler2_mir::driver_form(db, *function))
                    .map_or(DeclRef::Source(*function), DeclRef::Source),
                DeclRef::External(_) => *function,
            };
            if let Some(native) = crate::natives::of(db, function) {
                return native.callable().map(|_| Callee::Native(native));
            }
            match &function {
                DeclRef::Source(function) => {
                    let arguments = args
                        .iter()
                        .take(ntypeargs)
                        .map(|argument| {
                            let template = body.type_value(argument).ok_or_else(|| {
                                Blocker::new(
                                    "call with type arguments",
                                    "that are not types loaded once",
                                )
                            })?;
                            Ok(type_argument(template_ty(db, template, body.arguments)))
                        })
                        .collect::<Result<_, Blocker>>()?;
                    Ok(Callee::Function(
                        FnKey::new(*function, Entry::Direct).with_arguments(arguments),
                    ))
                }
                DeclRef::External(_) => Err(Blocker::new(
                    "call of a function without source",
                    function_display_name(db, function),
                )),
            }
        }
        Operand::Constant(Constant::GenericFunction { func, .. }) => Err(Blocker::new(
            "call of a generic function",
            function_display_name(db, *func),
        )),
        Operand::Constant(_) => Err(Blocker::new("call of a non-function constant", "")),
        Operand::Copy(_) | Operand::Move(_) => Ok(Callee::Value),
    }
}

/// The types a call of `native` names for its type parameters: the call's
/// first `ntypeargs` arguments.
pub(crate) fn native_type_arguments<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    native: &crate::natives::Native,
    args: &[Operand<'db>],
    ntypeargs: usize,
) -> Result<Vec<NativeTy<'db>>, Blocker> {
    let unsupported = |detail: &str| {
        Blocker::new(
            "generic standard library native",
            format!("{}: {detail}", native.path),
        )
    };
    // A method of `Map` is called with no type arguments: its key and
    // value types are those of its receiver.
    if ntypeargs == 0 && native.type_args == 2 && native.path.starts_with("baml.Map.") {
        return match args
            .first()
            .map(|map| operand_ty(db, body, map))
            .transpose()?
        {
            Some(map) => match narrowed(map) {
                NativeTy::Map(key, value) => Ok(vec![*key, *value]),
                other => Err(unsupported(&format!("on {}", describe(&other)))),
            },
            None => Err(unsupported("called with no receiver")),
        };
    }
    if ntypeargs != native.type_args || args.len() < ntypeargs {
        return Err(unsupported("called with other type arguments"));
    }
    let types = args[..ntypeargs]
        .iter()
        .map(|argument| {
            let template = body
                .type_value(argument)
                .ok_or_else(|| unsupported("a type argument that is not a type loaded once"))?;
            match template_ty(db, template, body.arguments) {
                // `baml.json` returns a value of a type parameter as a
                // union holds it, whatever the type.
                Some(ty) if ty != NativeTy::Null && crate::json::takes_descriptions(native) => {
                    Ok(ty)
                }
                // A native moves a value of a type parameter as one word.
                Some(ty) if ty != NativeTy::Null && !ty.is_pair() => Ok(ty),
                Some(ty) => Err(unsupported(&format!("at {}", describe(&ty)))),
                None => Err(unsupported("at a type with no native representation")),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    // The elements' own `to_string` would be a call back into compiled
    // code, which the runtime does not make.
    if native.path == "baml.Array.join"
        && !matches!(
            types.as_slice(),
            [NativeTy::String | NativeTy::Int | NativeTy::Float | NativeTy::Bool]
        )
    {
        return Err(Blocker::new(
            "Array.join of elements with their own to_string",
            types.first().map(describe).unwrap_or_default(),
        ));
    }
    Ok(types)
}

/// How an interface method call finds its code.
pub(crate) enum Virtual<'db> {
    /// An operator of `baml.ops` on an `int`, `float` or `bool`: the
    /// instructions the operator itself compiles to, with no call.
    Operator(Operator),
    /// The receiver's type is known where the call is written.
    Static(Target<'db>),
    /// The receiver is some implementor: its type id picks the code.
    Dynamic(Slot),
}

/// What a method of a `baml.ops` interface computes on primitives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Operator {
    Binary(BinOp),
    Unary(UnaryOp),
}

impl Operator {
    /// The operator `method` of `interface` is on a receiver of type
    /// `receiver`, when that is an `int`, a `float` or a `bool`.
    fn of(
        db: &dyn baml_compiler2_mir::Db,
        interface: &TyTemplateInterface,
        method: &str,
        receiver: &NativeTy<'_>,
    ) -> Option<Self> {
        if !matches!(receiver, NativeTy::Int | NativeTy::Float | NativeTy::Bool) {
            return None;
        }
        // `baml.ops`: the standard library's operator interfaces.
        let name = &interface.name;
        if name.key().kind(db) != baml_base::SourceRootKind::Stdlib
            || !matches!(name.namespace().as_slice(), [namespace] if namespace.as_str() == "ops")
        {
            return None;
        }
        let operator = match (interface.name.name().as_str(), method) {
            ("Add", "add") => Self::Binary(BinOp::Add),
            ("Subtract", "sub") => Self::Binary(BinOp::Sub),
            ("Multiply", "mul") => Self::Binary(BinOp::Mul),
            ("Divide", "div") => Self::Binary(BinOp::Div),
            ("Remainder", "rem") => Self::Binary(BinOp::Mod),
            ("Negate", "neg") => Self::Unary(UnaryOp::Neg),
            ("BitAnd", "bit_and") => Self::Binary(BinOp::BitAnd),
            ("BitOr", "bit_or") => Self::Binary(BinOp::BitOr),
            ("BitXor", "bit_xor") => Self::Binary(BinOp::BitXor),
            ("ShiftLeft", "shl") => Self::Binary(BinOp::Shl),
            ("ShiftRight", "shr") => Self::Binary(BinOp::Shr),
            ("Equals", "eq") => Self::Binary(BinOp::Eq),
            ("Equals", "neq") => Self::Binary(BinOp::Ne),
            _ => return None,
        };
        Some(operator)
    }

    /// The type of the result on operands of these types.
    pub(crate) fn result<'db>(self, operands: &[NativeTy<'db>]) -> Result<NativeTy<'db>, Blocker> {
        match (self, operands) {
            (Self::Binary(op), [left, right]) => {
                // The other operand is a primitive too: a `bigint` is not.
                if !matches!(right, NativeTy::Int | NativeTy::Float | NativeTy::Bool) {
                    return Err(Blocker::new(
                        "operator method on these operand types",
                        format!("{op}"),
                    ));
                }
                binary_typing(op, left, right).map(|(_, result)| result)
            }
            (Self::Unary(op), [operand]) => unary_typing(op, operand),
            _ => Err(Blocker::new("operator method call", "with other arguments")),
        }
    }
}

/// The standard library's sleep: its one argument is a `bigint` count of
/// nanoseconds.
const SLEEP: &str = "baml.sys._sleep";

pub(crate) fn virtual_target<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    interface: &TyTemplateInterface,
    method: &str,
    args: &[Operand<'db>],
    ntypeargs: usize,
    self_arg: usize,
) -> Result<Virtual<'db>, Blocker> {
    let unsupported =
        |detail: &str| Blocker::new("interface method call", format!("{method}: {detail}"));
    if ntypeargs != 0 {
        return Err(unsupported("with type arguments"));
    }
    let receiver = args
        .get(self_arg)
        .ok_or_else(|| unsupported("without a receiver"))?;
    let receiver = operand_ty(db, body, receiver)?;
    if self_arg == 0
        && let Some(operator) = Operator::of(db, interface, method, &receiver)
    {
        return Ok(Virtual::Operator(operator));
    }
    if receiver.is_boxed() {
        if self_arg != 0 {
            return Err(unsupported("dispatched on a later argument"));
        }
        return Ok(Virtual::Dynamic(Slot::new(interface, method)));
    }
    match dispatch::resolve(db, &receiver, interface, method, body.arguments)? {
        Some(target) => Ok(Virtual::Static(target)),
        None => Err(unsupported("on a type with no implementation found")),
    }
}

/// An interface method call whose receiver is only known when it runs.
pub(crate) struct Dispatched<'db> {
    pub(crate) slot: Slot,
    pub(crate) interface: TyTemplateInterface,
    pub(crate) method: String,
    /// The types of the arguments after the receiver, and of the place the
    /// result goes.
    pub(crate) arguments: Vec<NativeTy<'db>>,
    pub(crate) result: NativeTy<'db>,
}

/// Whether a call with these value arguments, storing a `result`, matches
/// the layout of the copy of the function it calls.
fn call_fits<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    key: &FnKey<'db>,
    args: &[Operand<'db>],
    result: &NativeTy<'db>,
) -> Result<(), Blocker> {
    // An argument left to its default has whatever type its parameter has.
    let found = args
        .iter()
        .map(|argument| {
            if crate::types::is_omitted(argument) {
                Ok(None)
            } else {
                operand_ty(db, body, argument).map(Some)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    // A callee outside the subset is reported as its own blocked function;
    // only a callee that will be compiled constrains the call.
    if let Ok(expected) = function_signature(db, key) {
        if expected
            .params
            .iter()
            .zip(&found)
            .any(|(expected, found)| found.is_none() && !crate::types::has_omitted_form(expected))
        {
            return Err(Blocker::new("defaulted argument", "of an int or float"));
        }
        let fits = expected.params.len() == found.len()
            && expected.params.iter().zip(&found).all(|(expected, found)| {
                found
                    .as_ref()
                    .is_none_or(|found| assignable(expected, found))
            })
            && assignable(result, &expected.result);
        if !fits {
            return Err(Blocker::new(
                "call that does not match its callee's layout",
                function_display_name(db, DeclRef::Source(key.loc)),
            ));
        }
    }
    Ok(())
}

/// What a function needs from the rest of the program, and what keeps it
/// from being compiled.
pub(crate) struct Checked<'db> {
    /// The interface method calls it dispatches when it runs.
    pub(crate) dispatched: Vec<Dispatched<'db>>,
    /// The types it makes values of that an interface value can hold.
    pub(crate) constructed: Vec<NativeTy<'db>>,
    /// Every unsupported construct, in block order, without repeats.
    pub(crate) blockers: Vec<Blocker>,
    /// The functions it calls directly or makes values of.
    pub(crate) needed: Vec<Needed<'db>>,
}

pub(crate) fn check_function<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    key: &FnKey<'db>,
    mir: &MirFunction<'db>,
    captures: &[NativeTy<'db>],
) -> Checked<'db> {
    let mut checker = Checker {
        db,
        key,
        blockers: Vec::new(),
        needed: Vec::new(),
        dispatched: Vec::new(),
        constructed: Vec::new(),
    };
    match body_of(mir) {
        Ok(body) => {
            checker.note(signature(db, mir, body, &key.arguments).map(drop));
            if key.entry == Entry::Bound {
                checker.note(bound_signature(db, key.loc).map(drop));
            }
            let body = &Body::new(db, mir, body, captures, key);
            let loops = ArrayLoops::of(db, body);
            for (index, block) in body.blocks.iter().enumerate() {
                if !body.is_reachable(index) {
                    continue;
                }
                body.enter(index);
                checker.note(type_tags_switched(body, index));
                for statement in &block.statements {
                    checker.statement_needs(body, &statement.kind);
                    checker.note(statement_supported(db, body, &loops, &statement.kind));
                }
                match &block.terminator {
                    Some(terminator) => checker.terminator(body, &loops, terminator),
                    None => checker.note(Err(Blocker::new(
                        "unterminated block",
                        block.id.to_string(),
                    ))),
                }
            }
        }
        Err(blocker) => checker.note(Err(blocker)),
    }
    Checked {
        blockers: checker.blockers,
        needed: checker.needed,
        dispatched: checker.dispatched,
        constructed: checker.constructed,
    }
}

struct Checker<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    key: &'a FnKey<'db>,
    blockers: Vec<Blocker>,
    needed: Vec<Needed<'db>>,
    dispatched: Vec<Dispatched<'db>>,
    constructed: Vec<NativeTy<'db>>,
}

impl<'db> Checker<'_, 'db> {
    fn construct(&mut self, ty: NativeTy<'db>) {
        if !self.constructed.contains(&ty) {
            self.constructed.push(ty);
        }
    }

    fn need(&mut self, key: FnKey<'db>, captures: Vec<NativeTy<'db>>) {
        if !self
            .needed
            .iter()
            .any(|needed| needed.key == key && needed.captures == captures)
        {
            self.needed.push(Needed { key, captures });
        }
    }

    /// A function named where a value is expected is needed as one.
    fn operand_needs(&mut self, operand: &Operand<'db>) {
        match operand {
            Operand::Constant(Constant::Function(DeclRef::Source(function))) => {
                self.need(FnKey::new(*function, Entry::Value), Vec::new());
            }
            Operand::Constant(Constant::EnumVariant { enum_ref, .. }) => {
                self.construct(NativeTy::Enum(*enum_ref));
            }
            _ => {}
        }
    }

    /// The functions a statement makes values of.
    fn statement_needs(&mut self, body: &Body<'_, 'db>, statement: &StatementKind<'db>) {
        let StatementKind::Assign { value, .. } = statement else {
            return;
        };
        match value {
            Rvalue::Use(operand) => self.operand_needs(operand),
            Rvalue::Map(_, _, entries) => {
                for (key, value) in entries {
                    self.operand_needs(key);
                    self.operand_needs(value);
                }
            }
            Rvalue::Array(_, operands)
            | Rvalue::Aggregate {
                fields: operands, ..
            } => {
                for operand in operands {
                    self.operand_needs(operand);
                }
                if let Rvalue::Aggregate {
                    kind:
                        AggregateKind::Class {
                            class,
                            type_arg_templates,
                            ..
                        },
                    ..
                } = value
                {
                    self.construct(NativeTy::Class(
                        *class,
                        type_arg_templates
                            .iter()
                            .map(|template| template_ty(self.db, template, body.arguments))
                            .collect(),
                    ));
                }
            }
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } => {
                // A capture that is not a cell is reported by the statement's
                // own check.
                let captures = captures
                    .iter()
                    .map(|capture| match operand_ty(self.db, body, capture) {
                        Ok(NativeTy::Cell(content)) => Some(*content),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(captures) = captures {
                    self.need(self.key.lambda(*lambda_idx), captures);
                }
            }
            Rvalue::MakeBoundMethod {
                func: DeclRef::Source(function),
                ..
            } => self.need(FnKey::new(*function, Entry::Bound), Vec::new()),
            _ => {}
        }
    }

    fn note(&mut self, outcome: Result<(), Blocker>) {
        if let Err(blocker) = outcome
            && !self
                .blockers
                .iter()
                .any(|seen| seen.construct == blocker.construct)
        {
            self.blockers.push(blocker);
        }
    }

    fn terminator(
        &mut self,
        body: &Body<'_, 'db>,
        loops: &ArrayLoops<'db>,
        terminator: &Terminator<'db>,
    ) {
        if let Terminator::Call {
            callee,
            args,
            ntypeargs,
            ..
        } = terminator
        {
            match call_target(self.db, body, callee, args, *ntypeargs) {
                Ok(Callee::Function(key)) => self.need(key, Vec::new()),
                // What `baml.json` decodes is made by the runtime.
                Ok(Callee::Native(native)) => {
                    let made = crate::json::check_call(self.db, body, native, args, *ntypeargs);
                    for described in made.unwrap_or_default() {
                        self.construct(match described {
                            crate::json::Described::Class(class) => {
                                NativeTy::Class(class, Vec::new())
                            }
                            crate::json::Described::Enum(enum_ref) => NativeTy::Enum(enum_ref),
                        });
                    }
                }
                _ => {}
            }
            for argument in args {
                self.operand_needs(argument);
            }
        }
        let in_loop = loops.start(terminator).is_some() || loops.step(terminator).is_some();
        if let Terminator::VirtualCall {
            iface,
            method,
            args,
            ntypeargs,
            self_arg,
            destination,
            ..
        } = terminator
            && !in_loop
        {
            match virtual_target(self.db, body, iface, method, args, *ntypeargs, *self_arg) {
                Ok(Virtual::Static(Target::Function(key))) => self.need(key, Vec::new()),
                Ok(Virtual::Dynamic(slot)) => {
                    let arguments = args
                        .iter()
                        .skip(1)
                        .map(|argument| operand_ty(self.db, body, argument))
                        .collect::<Result<Vec<_>, _>>();
                    let result =
                        place_local(destination).and_then(|local| local_ty(self.db, body, local));
                    if let (Ok(arguments), Ok(result)) = (arguments, result) {
                        self.dispatched.push(Dispatched {
                            slot,
                            interface: iface.clone(),
                            method: method.clone(),
                            arguments,
                            result,
                        });
                    }
                }
                _ => {}
            }
            for argument in args {
                self.operand_needs(argument);
            }
        }
        let outcome = if in_loop {
            Ok(())
        } else {
            terminator_supported(self.db, body, terminator)
        };
        self.note(outcome);
    }
}

fn rvalue_name(value: &Rvalue<'_>) -> &'static str {
    match value {
        Rvalue::Use(_) => "use",
        Rvalue::BinaryOp { .. } => "binary operator",
        Rvalue::UnaryOp { .. } => "unary operator",
        Rvalue::TraceHookSettings { .. } => "trace hook settings",
        Rvalue::ConcatStrings(_) => "string concatenation",
        Rvalue::Array(..)
        | Rvalue::Aggregate {
            kind: AggregateKind::Array,
            ..
        } => "array construction",
        Rvalue::Uint8Array(_) => "byte array construction",
        Rvalue::Map(..) => "map construction",
        Rvalue::Aggregate {
            kind: AggregateKind::Class { .. },
            ..
        } => "class instance construction",
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple { .. },
            ..
        } => "tuple construction",
        Rvalue::Discriminant(_) => "enum discriminant read",
        Rvalue::TypeTag(_) => "type tag read",
        Rvalue::Len(_) => "length",
        Rvalue::IsType { .. } | Rvalue::IsTypeTag { .. } => "type test",
        Rvalue::SatisfiesBound { .. } => "interface bound test",
        Rvalue::MakeClosure { .. } => "closure construction",
        Rvalue::MakeBoundMethod { .. } | Rvalue::MakeVirtualBoundMethod { .. } => {
            "bound method value"
        }
        Rvalue::MakeVirtualFunction { .. }
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. } => "function value",
        Rvalue::VirtualFieldAccess { .. } => "interface field access",
        Rvalue::LoadType(_) => "type value",
        Rvalue::CurrentPackage => "package value",
    }
}

/// Each operand can be stored where `expected` is declared.
fn all_assignable<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    operands: &[Operand<'db>],
    mut expected: impl FnMut(usize) -> Option<NativeTy<'db>>,
    what: &str,
) -> Result<(), Blocker> {
    for (index, operand) in operands.iter().enumerate() {
        let found = operand_ty(db, body, operand)?;
        let fits = expected(index).is_some_and(|expected| assignable(&expected, &found));
        if !fits {
            return Err(Blocker::new(
                format!("{what} of an unexpected type"),
                describe(&found),
            ));
        }
    }
    Ok(())
}

/// The type an rvalue evaluates to when it is stored where `expected` is
/// declared. Only an array literal needs the expectation: its element type
/// is the destination's.
pub(crate) fn rvalue_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    loops: &ArrayLoops<'db>,
    value: &Rvalue<'db>,
    expected: &NativeTy<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    // What `next()` answered is read in two ways: as the element, and
    // through the test for the end of the iteration.
    match value {
        Rvalue::Use(operand) => {
            if let Some(element) = operand_local(operand).and_then(|local| loops.item(local)) {
                return Ok(element.clone());
            }
        }
        Rvalue::IsType { operand, test }
            if operand_local(operand).is_some_and(|local| loops.item(local).is_some())
                && is_done_test(db, test) =>
        {
            return Ok(NativeTy::Bool);
        }
        _ => {}
    }
    match value {
        Rvalue::Use(operand) => operand_ty(db, body, operand),
        // Whether a parameter was left to its default.
        Rvalue::BinaryOp {
            op: BinOp::Eq,
            left,
            right,
        } if crate::types::is_omitted(right) => {
            if crate::types::has_omitted_form(&operand_ty(db, body, left)?) {
                Ok(NativeTy::Bool)
            } else {
                Err(Blocker::new("defaulted argument", "of an int or float"))
            }
        }
        Rvalue::BinaryOp { op, left, right } => {
            let left = operand_ty(db, body, left)?;
            let right = operand_ty(db, body, right)?;
            binary_typing(*op, &left, &right).map(|(_, result)| result)
        }
        Rvalue::UnaryOp { op, operand } => unary_typing(*op, &operand_ty(db, body, operand)?),
        Rvalue::ConcatStrings(parts) => {
            all_assignable(db, body, parts, |_| Some(NativeTy::String), "string part")?;
            Ok(NativeTy::String)
        }
        // An enum value is its variant's position already.
        Rvalue::Discriminant(place) => match narrowed(place_ty(db, body, place)?) {
            NativeTy::Enum(_) => Ok(NativeTy::Int),
            other => Err(Blocker::new(
                "discriminant of a non-enum value",
                describe(&other),
            )),
        },
        Rvalue::TypeTag(place) => match place_ty(db, body, place)? {
            boxed if boxed.is_boxed() => Ok(NativeTy::Int),
            other => Err(Blocker::new(
                "type tag read",
                format!("of {}", describe(&other)),
            )),
        },
        Rvalue::Len(place) => match narrowed(place_ty(db, body, place)?) {
            NativeTy::Array(_) | NativeTy::Map(..) => Ok(NativeTy::Int),
            other => Err(Blocker::new(
                "length of a non-array value",
                describe(&other),
            )),
        },
        Rvalue::Map(_, _, entries) => {
            let map = match expected {
                NativeTy::Optional(inner) => inner,
                other => other,
            };
            let NativeTy::Map(key, value) = map else {
                return Err(Blocker::new(
                    "map construction where no map is expected",
                    describe(expected),
                ));
            };
            let (keys, values): (Vec<_>, Vec<_>) = entries.iter().cloned().unzip();
            all_assignable(db, body, &keys, |_| Some((**key).clone()), "map key")?;
            all_assignable(db, body, &values, |_| Some((**value).clone()), "map value")?;
            Ok(map.clone())
        }
        Rvalue::Array(_, elements)
        | Rvalue::Aggregate {
            kind: AggregateKind::Array,
            fields: elements,
        } => {
            let array = match expected {
                NativeTy::Optional(inner) => inner,
                other => other,
            };
            let NativeTy::Array(element) = array else {
                return Err(Blocker::new(
                    "array construction where no array is expected",
                    describe(expected),
                ));
            };
            all_assignable(
                db,
                body,
                elements,
                |_| Some((**element).clone()),
                "array element",
            )?;
            Ok(array.clone())
        }
        Rvalue::Aggregate {
            kind:
                AggregateKind::Class {
                    class,
                    type_arg_templates,
                    finalizable,
                },
            fields,
        } => {
            if *finalizable {
                return Err(Blocker::new("construction of a class with a finalizer", ""));
            }
            let arguments: Vec<_> = type_arg_templates
                .iter()
                .map(|template| template_ty(db, template, body.arguments))
                .collect();
            let layout = class_layout(db, *class, &arguments)?;
            if layout.len() != fields.len() {
                return Err(Blocker::new(
                    "class instance construction that does not match the layout",
                    "",
                ));
            }
            all_assignable(
                db,
                body,
                fields,
                |index| layout.get(index).cloned(),
                "field value",
            )?;
            Ok(NativeTy::Class(*class, arguments))
        }
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple { element_templates },
            fields,
        } => {
            let elements = element_templates
                .iter()
                .map(|template| template_ty(db, template, body.arguments))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Blocker::new("tuple construction", "of an unsupported element"))?;
            if elements.len() != fields.len() {
                return Err(Blocker::new(
                    "tuple construction",
                    "that does not match its type",
                ));
            }
            all_assignable(
                db,
                body,
                fields,
                |index| elements.get(index).cloned(),
                "tuple element",
            )?;
            Ok(NativeTy::Tuple(elements))
        }
        Rvalue::IsType { operand, test } => {
            let operand = operand_ty(db, body, operand)?;
            type_test_plan(db, &operand, TestKind::of(db, test, body.arguments), false)?;
            Ok(NativeTy::Bool)
        }
        Rvalue::IsTypeTag { operand, tag } => {
            let operand = operand_ty(db, body, operand)?;
            type_test_plan(db, &operand, TestKind::of_tag(*tag), true)?;
            Ok(NativeTy::Bool)
        }
        Rvalue::MakeClosure {
            lambda_idx,
            captures,
            ..
        } => {
            // A lambda is compiled for its function's type arguments. What
            // it cannot have is a type id passed to it.
            if identified_arguments(db, &body.key.lambda(*lambda_idx))
                .iter()
                .any(|needed| *needed)
            {
                return Err(Blocker::new("closure that tests a type argument", ""));
            }
            for capture in captures {
                if !matches!(operand_ty(db, body, capture)?, NativeTy::Cell(_)) {
                    return Err(Blocker::new("closure capture that is not a cell", ""));
                }
            }
            let lambda = body
                .mir
                .lambdas
                .get(*lambda_idx)
                .ok_or_else(|| Blocker::new("closure of a lambda that does not exist", ""))?;
            // A lambda outside the subset is reported as its own blocked
            // function.
            let signature = signature(db, lambda, body_of(lambda)?, body.arguments)
                .map_err(|_| Blocker::new("closure of a lambda outside the subset", ""))?;
            Ok(NativeTy::Function(Box::new(signature)))
        }
        Rvalue::MakeBoundMethod {
            func,
            receiver,
            frame,
        } => {
            let DeclRef::Source(function) = func else {
                return Err(Blocker::new(
                    "bound method value",
                    function_display_name(db, *func),
                ));
            };
            if !matches!(frame, BoundFrame::Receiver) {
                return Err(Blocker::new("bound method value", "with type arguments"));
            }
            let (expected, signature) = bound_signature(db, *function)?;
            let found = operand_ty(db, body, receiver)?;
            if !found.is_ref() || !assignable(&expected, &found) {
                return Err(Blocker::new("bound method value", describe(&found)));
            }
            Ok(NativeTy::Function(Box::new(signature)))
        }
        other => Err(Blocker::new(rvalue_name(other), "")),
    }
}

/// A local of type `type` holds a type argument. Native code is compiled per
/// concrete type, so the value is never needed: the local is supported only
/// as an argument of an intrinsic, which ignores it.
fn is_type_value(body: &Body<'_, '_>, place: &Place) -> bool {
    matches!(place, Place::Local(local) if matches!(body.local(*local).ty, RuntimeTy::Type))
}

fn statement_supported<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    loops: &ArrayLoops<'db>,
    statement: &StatementKind<'db>,
) -> Result<(), Blocker> {
    match statement {
        StatementKind::Nop => Ok(()),
        StatementKind::Assign {
            destination,
            value: Rvalue::LoadType(_),
        } if is_type_value(body, destination) => Ok(()),
        StatementKind::Assign { destination, value } => {
            let destination = place_ty(db, body, destination)?;
            let value = rvalue_ty(db, body, loops, value, &destination)?;
            if assignable(&destination, &value) {
                Ok(())
            } else {
                Err(Blocker::new(
                    "assignment between different types",
                    format!("{} to {}", describe(&value), describe(&destination)),
                ))
            }
        }
        // Nothing is freed eagerly: the collector reclaims what is dropped.
        StatementKind::Drop(place) => {
            let loop_local = matches!(place, Place::Local(local)
                if loops.iterator(*local).is_some() || loops.item(*local).is_some());
            if loop_local || is_type_value(body, place) {
                Ok(())
            } else {
                place_ty(db, body, place).map(drop)
            }
        }
        StatementKind::FreshCell { local, .. } => match local_ty(db, body, *local)? {
            NativeTy::Cell(_) => Ok(()),
            _ => Err(Blocker::new("fresh cell for a local that has none", "")),
        },
        StatementKind::Intrinsic { op, .. } => {
            Err(Blocker::new("compiler intrinsic", format!("{op:?}")))
        }
        StatementKind::VirtualFieldStore { .. } => Err(Blocker::new("interface field access", "")),
    }
}

fn expect_bool(found: &NativeTy<'_>, what: &str) -> Result<(), Blocker> {
    if *found == NativeTy::Bool {
        Ok(())
    } else {
        Err(Blocker::new(
            format!("{what} that is not a bool"),
            describe(found),
        ))
    }
}

/// How a type test of a value of a known static type is decided.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TypeTestPlan<'db> {
    /// By the static type alone.
    Always(bool),
    /// By whether an optional holds a value (`true`) or `null` (`false`).
    Present(bool),
    /// By the type id in the header of the object a union's reference
    /// names; `null` fails.
    Header(TestKind<'db>),
}

/// How `test` is decided for an operand of type `operand`. `coarse` says the
/// test asks for any array rather than an array of one element type.
pub(crate) fn type_test_plan<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    operand: &NativeTy<'db>,
    test: Option<TestKind<'db>>,
    coarse: bool,
) -> Result<TypeTestPlan<'db>, Blocker> {
    let test = test.ok_or_else(|| Blocker::new("type test", "of a type without one type id"))?;
    Ok(match operand {
        NativeTy::Union(_) | NativeTy::Unknown => {
            match &test {
                TestKind::Exactly(NativeTy::Null) => return Ok(TypeTestPlan::Present(false)),
                TestKind::Exactly(NativeTy::Class(class, arguments)) => {
                    class_layout(db, *class, arguments)?;
                }
                TestKind::AnyOf(tests) => {
                    for test in tests {
                        if !matches!(test, TestKind::Exactly(NativeTy::Null)) {
                            type_test_plan(db, operand, Some(test.clone()), coarse)?;
                        }
                    }
                }
                TestKind::Exactly(_)
                | TestKind::Argument(_)
                | TestKind::Implements(_)
                | TestKind::Panic => {}
                // Arrays of different element types share their type ids.
                TestKind::AnyArray => {
                    let arrays = match operand {
                        NativeTy::Union(members) => members
                            .iter()
                            .filter(|member| matches!(member, NativeTy::Array(_)))
                            .count(),
                        _ => usize::MAX,
                    };
                    if !coarse && arrays > 1 {
                        return Err(Blocker::new("type test", "between array types"));
                    }
                }
            }
            TypeTestPlan::Header(test)
        }
        _ if !test.is_static() => {
            return Err(Blocker::new(
                "type test",
                "of a type argument or an interface, on a value that is not a union",
            ));
        }
        NativeTy::Optional(inner) => match (test.matches(&NativeTy::Null), test.matches(inner)) {
            (true, true) => TypeTestPlan::Always(true),
            (true, false) => TypeTestPlan::Present(false),
            (false, true) => TypeTestPlan::Present(true),
            (false, false) => TypeTestPlan::Always(false),
        },
        other => TypeTestPlan::Always(test.matches(other)),
    })
}

/// The type of the value a narrowing bind stores where its test passes.
pub(crate) fn narrow_bound<'db>(source: &NativeTy<'db>, plan: &TypeTestPlan<'db>) -> NativeTy<'db> {
    match (source, plan) {
        (NativeTy::Optional(inner), TypeTestPlan::Present(true)) => (**inner).clone(),
        (_, TypeTestPlan::Present(false)) => NativeTy::Null,
        (_, TypeTestPlan::Header(TestKind::Exactly(ty))) => ty.clone(),
        _ => source.clone(),
    }
}

fn terminator_supported<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    terminator: &Terminator<'db>,
) -> Result<(), Blocker> {
    match terminator {
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => Ok(()),
        Terminator::Branch { condition, .. } => {
            expect_bool(&operand_ty(db, body, condition)?, "branch condition")
        }
        Terminator::Switch {
            discriminant, arms, ..
        } => {
            if operand_ty(db, body, discriminant)? != NativeTy::Int {
                return Err(Blocker::new("switch on a non-int value", ""));
            }
            let on_tag = body
                .current()
                .is_some_and(|block| switched_tag(body, block).is_some());
            for (key, _) in arms {
                match key {
                    SwitchKey::Int(_) if !on_tag => {}
                    // A class's tag is the type id of its instances, which
                    // a class with type parameters has several of.
                    SwitchKey::Class(class)
                        if on_tag
                            && baml_compiler2_hir_ty::layout::class_generic_params(db, *class)
                                .is_empty() =>
                    {
                        class_layout(db, *class, &[])?;
                    }
                    SwitchKey::Int(tag)
                        if TestKind::of_tag(*tag)
                            .is_some_and(|test| test.is_static() || test == TestKind::Panic) => {}
                    _ => return Err(Blocker::new("switch on a class tag", "")),
                }
            }
            Ok(())
        }
        Terminator::NarrowBind {
            source,
            test,
            destination,
            ..
        } => {
            let source = operand_ty(db, body, source)?;
            let plan = type_test_plan(db, &source, TestKind::of(db, test, body.arguments), false)?;
            let destination = local_ty(db, body, *destination)?;
            if assignable(&destination, &narrow_bound(&source, &plan)) {
                Ok(())
            } else {
                Err(Blocker::new("type test", "narrowing bind to another type"))
            }
        }
        Terminator::Call {
            has_trace,
            callee,
            args,
            ntypeargs,
            destination,
            ..
        } => {
            let called = call_target(db, body, callee, args, *ntypeargs)?;
            if *has_trace {
                return Err(Blocker::new("call with a trace attachment", ""));
            }
            let result = local_ty(db, body, place_local(destination)?)?;
            match called {
                Callee::Value => {
                    let NativeTy::Function(expected) = narrowed(operand_ty(db, body, callee)?)
                    else {
                        return Err(Blocker::new("call of a value that is not a function", ""));
                    };
                    if *ntypeargs != 0 {
                        return Err(Blocker::new("call with type arguments", "function value"));
                    }
                    let fits = expected.params.len() == args.len()
                        && assignable(&result, &expected.result);
                    if !fits {
                        return Err(Blocker::new(
                            "call that does not match its callee's layout",
                            "function value",
                        ));
                    }
                    all_assignable(
                        db,
                        body,
                        args,
                        |index| expected.params.get(index).cloned(),
                        "argument",
                    )
                }
                Callee::Intrinsic(intrinsic) => {
                    // Leading type arguments carry nothing native code needs.
                    let found = args
                        .iter()
                        .skip(*ntypeargs)
                        .map(|argument| operand_ty(db, body, argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    match intrinsic.result(&found) {
                        Some(produced) if assignable(&result, &produced) => Ok(()),
                        _ => Err(Blocker::new(
                            "builtin call on these argument types",
                            format!("{intrinsic:?}"),
                        )),
                    }
                }
                Callee::Native(native) => {
                    let (signature, _) = native.callable()?;
                    let types = native_type_arguments(db, body, native, args, *ntypeargs)?;
                    crate::json::check_call(db, body, native, args, *ntypeargs)?;
                    let values = &args[*ntypeargs..];
                    if values.len() != signature.params.len()
                        || !assignable(&result, &signature.result.native(db, &types))
                    {
                        return Err(Blocker::new(
                            "call that does not match its callee's layout",
                            native.path,
                        ));
                    }
                    all_assignable(
                        db,
                        body,
                        values,
                        |index| signature.params.get(index).map(|ty| ty.native(db, &types)),
                        "argument",
                    )
                }
                Callee::Function(key) => {
                    let name = || function_display_name(db, DeclRef::Source(key.loc));
                    // The callee is told the type id of each type argument
                    // it tests values against.
                    for (slot, uses) in identified(db, &key, &mut Vec::new(), &mut Vec::new())
                        .into_iter()
                        .enumerate()
                        .filter(|(_, uses)| *uses != 0)
                    {
                        let known = match body.type_value(&args[slot]) {
                            Some(TyTemplate::TypeArgRef(own)) => body
                                .arguments
                                .get(*own as usize)
                                .is_some_and(Option::is_some),
                            Some(template)
                                if template_ty(db, template, body.arguments)
                                    .is_some_and(|ty| has_type_id(&ty)) =>
                            {
                                true
                            }
                            // `baml.json` takes any type it has a
                            // description of.
                            Some(template)
                                if uses & TESTED == 0 && template.is_fully_concrete() =>
                            {
                                crate::json::JsonTypes::new()
                                    .describe(db, &template.substitute_symbolic(&[]))?;
                                true
                            }
                            _ => false,
                        };
                        if !known {
                            return Err(Blocker::new(
                                "call with type arguments",
                                format!("{}: a type without one type id", name()),
                            ));
                        }
                    }
                    call_fits(db, body, &key, &args[*ntypeargs..], &result)
                }
            }
        }
        Terminator::VirtualCall {
            has_trace,
            iface,
            method,
            args,
            ntypeargs,
            self_arg,
            destination,
            ..
        } => {
            if *has_trace {
                return Err(Blocker::new("call with a trace attachment", ""));
            }
            let result = local_ty(db, body, place_local(destination)?)?;
            match virtual_target(db, body, iface, method, args, *ntypeargs, *self_arg)? {
                Virtual::Operator(operator) => {
                    let found = args
                        .iter()
                        .map(|argument| operand_ty(db, body, argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    if assignable(&result, &operator.result(&found)?) {
                        Ok(())
                    } else {
                        Err(Blocker::new(
                            "operator method call",
                            format!("{method}: into another type"),
                        ))
                    }
                }
                Virtual::Static(Target::ToString) => {
                    let found = operand_ty(db, body, &args[*self_arg])?;
                    match Intrinsic::ToString.result(&[found]) {
                        Some(produced) if assignable(&result, &produced) => Ok(()),
                        _ => Err(Blocker::new(
                            "builtin call on these argument types",
                            "ToString",
                        )),
                    }
                }
                // The array itself, sorted in place.
                Virtual::Static(Target::Sort(_)) => {
                    let found = operand_ty(db, body, &args[*self_arg])?;
                    if assignable(&result, &found) {
                        Ok(())
                    } else {
                        Err(Blocker::new(
                            "interface method call",
                            format!("{method}: into another type"),
                        ))
                    }
                }
                Virtual::Static(Target::Function(key)) => {
                    if identified_arguments(db, &key).contains(&true) {
                        return Err(Blocker::new(
                            "interface method call",
                            format!("{method}: a default body that tests its type argument"),
                        ));
                    }
                    call_fits(db, body, &key, args, &result)
                }
                Virtual::Static(Target::Native(native)) => {
                    let (signature, _) = native.callable()?;
                    if *ntypeargs != 0
                        || native.type_args != 0
                        || args.len() != signature.params.len()
                        || !assignable(&result, &signature.result.native(db, &[]))
                    {
                        return Err(Blocker::new(
                            "call that does not match its callee's layout",
                            native.path,
                        ));
                    }
                    all_assignable(
                        db,
                        body,
                        args,
                        |index| signature.params.get(index).map(|ty| ty.native(db, &[])),
                        "argument",
                    )
                }
                // Checked against every implementation once the whole
                // program is known.
                Virtual::Dynamic(_) => args
                    .iter()
                    .try_for_each(|argument| operand_ty(db, body, argument).map(drop)),
            }
        }
        Terminator::SysOp {
            callee,
            args,
            destination,
            ..
        } => {
            let name = match callee {
                Operand::Constant(Constant::Function(function)) => {
                    function_display_name(db, *function)
                }
                _ => String::new(),
            };
            // The one system operation native code runs: a sleep, which
            // parks the task on the worker's timers.
            if name == SLEEP
                && let [delay] = args.as_slice()
                && operand_ty(db, body, delay)? == NativeTy::Bigint
                && assignable(&place_ty(db, body, destination)?, &NativeTy::Null)
            {
                Ok(())
            } else if let Some(native) =
                crate::natives::named(&name).filter(|native| native.flags.is_some())
            {
                // One the runtime implements is called as any native is.
                let (signature, _) = native.callable()?;
                let result = place_ty(db, body, destination)?;
                if args.len() != signature.params.len()
                    || !assignable(&result, &signature.result.native(db, &[]))
                {
                    return Err(Blocker::new(
                        "call that does not match its callee's layout",
                        native.path,
                    ));
                }
                all_assignable(
                    db,
                    body,
                    args,
                    |index| signature.params.get(index).map(|ty| ty.native(db, &[])),
                    "argument",
                )
            } else {
                Err(Blocker::new("system operation", name))
            }
        }
        Terminator::Spawn { plan, future, .. } => {
            let plan = narrowed(operand_ty(db, body, plan)?);
            let Some((_, started)) = crate::types::plan_body(db, &plan) else {
                return Err(Blocker::new("spawn", "of a plan with another layout"));
            };
            let expected = NativeTy::Future(Box::new(started.result));
            if assignable(&place_ty(db, body, future)?, &expected) {
                Ok(())
            } else {
                Err(Blocker::new("spawn", "into a future of another type"))
            }
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            let NativeTy::Future(value) = narrowed(place_ty(db, body, future)?) else {
                return Err(Blocker::new("await", "of a value that is not a future"));
            };
            let result = local_ty(db, body, place_local(destination)?)?;
            if assignable(&result, &value) {
                Ok(())
            } else {
                Err(Blocker::new("await", "into another type"))
            }
        }
        Terminator::AwaitAny { .. } => Err(Blocker::new("await of the first of several", "")),
        Terminator::Throw { value } => operand_ty(db, body, value).map(drop),
        Terminator::Rethrow { value, context } => {
            operand_ty(db, body, value)?;
            operand_ty(db, body, context).map(drop)
        }
        Terminator::ShortCircuit {
            operand,
            kind,
            destination,
            ..
        } => {
            if *kind == ShortCircuitKind::Coalesce {
                // `a ?? b`: where `a` is not null it is the result.
                let NativeTy::Optional(inner) = operand_ty(db, body, operand)? else {
                    return Err(Blocker::new("null coalescing of a non-optional value", ""));
                };
                let destination = place_ty(db, body, destination)?;
                return if assignable(&destination, &inner) {
                    Ok(())
                } else {
                    Err(Blocker::new("null coalescing into another type", ""))
                };
            }
            expect_bool(&operand_ty(db, body, operand)?, "short-circuit operand")?;
            expect_bool(&place_ty(db, body, destination)?, "short-circuit result")
        }
    }
}

/// The local the block `block` reads a type tag into and then switches on,
/// when that is how it ends: the one use of a type tag native code has an
/// answer for. The tag is the header itself, which only such a switch
/// knows how to read.
pub(crate) fn switched_tag(body: &Body<'_, '_>, block: usize) -> Option<Local> {
    let block = &body.blocks[block];
    let Some(Terminator::Switch {
        discriminant: Operand::Copy(Place::Local(read)) | Operand::Move(Place::Local(read)),
        ..
    }) = &block.terminator
    else {
        return None;
    };
    match &block.statements.last()?.kind {
        StatementKind::Assign {
            destination: Place::Local(tag),
            value: Rvalue::TypeTag(_),
        } if tag == read => Some(*tag),
        _ => None,
    }
}

/// A type tag is read only to be switched on where it is read; see
/// [`switched_tag`].
fn type_tags_switched(body: &Body<'_, '_>, index: usize) -> Result<(), Blocker> {
    use std::fmt::Write as _;

    let block = &body.blocks[index];
    let reads = block
        .statements
        .iter()
        .filter(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign {
                    value: Rvalue::TypeTag(_),
                    ..
                }
            )
        })
        .count();
    if reads == 0 {
        return Ok(());
    }
    let Some(tag) = switched_tag(body, index).filter(|_| reads == 1) else {
        return Err(Blocker::new("type tag read", "not switched on at once"));
    };
    // Nothing else names the local: it is written here and read by the
    // switch.
    let name = format!("{tag:?}");
    let mut uses = 0;
    for block in &body.blocks {
        let mut code = String::new();
        for statement in &block.statements {
            let _ = write!(code, "{:?};", statement.kind);
        }
        if let Some(Terminator::Switch { discriminant, .. }) = &block.terminator {
            let _ = write!(code, "{discriminant:?}");
        } else if let Some(terminator) = &block.terminator {
            let _ = write!(code, "{terminator:?}");
        }
        uses += code.matches(&name).count();
    }
    if uses == 2 {
        Ok(())
    } else {
        Err(Blocker::new("type tag read", "used as a value"))
    }
}
