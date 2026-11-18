//! Decides whether a MIR function is in the subset native code is generated
//! for, and names every construct that keeps it out.

use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_mir::{
    AggregateKind, Constant, MirFunction, MirFunctionBody, MirFunctionKind, Operand, OptLevel,
    Rvalue, ShortCircuitKind, StatementKind, SwitchKey, Terminator, function_display_name,
    lower_function,
};

use crate::{
    Blocker,
    scalar::{Scalar, binary_typing, local_scalar, operand_scalar, place_scalar, unary_typing},
};

/// The optimization level every function of a native program is lowered at.
pub(crate) const OPT: OptLevel = OptLevel::Two;

/// Parameter and result kinds of a function with a native calling convention.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Signature {
    pub(crate) params: Vec<Scalar>,
    pub(crate) result: Scalar,
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
pub(crate) fn signature(
    mir: &MirFunction<'_>,
    body: &MirFunctionBody<'_>,
) -> Result<Signature, Blocker> {
    let rename = |blocker: Blocker, what: &str| {
        Blocker::new(format!("{what} is a {}", blocker.construct), blocker.detail)
    };
    let result = local_scalar(body, baml_compiler2_mir::Local(0))
        .map_err(|blocker| rename(blocker, "result"))?;
    let params = (1..=mir.arity)
        .map(|index| {
            local_scalar(body, baml_compiler2_mir::Local(index))
                .map_err(|blocker| rename(blocker, "parameter"))
        })
        .collect::<Result<_, _>>()?;
    Ok(Signature { params, result })
}

/// The function a call names directly, when it is one written in BAML source.
pub(crate) fn direct_callee<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    callee: &Operand<'db>,
) -> Result<FunctionLoc<'db>, Blocker> {
    match callee {
        Operand::Constant(Constant::Function(DeclRef::Source(function))) => Ok(*function),
        Operand::Constant(Constant::Function(function @ DeclRef::External(_))) => {
            Err(Blocker::new(
                "call of a function without source",
                function_display_name(db, *function),
            ))
        }
        Operand::Constant(Constant::GenericFunction { func, .. }) => Err(Blocker::new(
            "call of a generic function",
            function_display_name(db, *func),
        )),
        Operand::Constant(_) => Err(Blocker::new("call of a non-function constant", "")),
        Operand::Copy(place) | Operand::Move(place) => {
            Err(Blocker::new("indirect call", place.to_string()))
        }
    }
}

/// What a function needs from the rest of the program, and what keeps it
/// from being compiled.
pub(crate) struct Checked<'db> {
    /// Every unsupported construct, in block order, without repeats.
    pub(crate) blockers: Vec<Blocker>,
    /// The source functions it calls directly.
    pub(crate) callees: Vec<FunctionLoc<'db>>,
}

pub(crate) fn check_function<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    mir: &MirFunction<'db>,
) -> Checked<'db> {
    let mut checker = Checker {
        db,
        blockers: Vec::new(),
        callees: Vec::new(),
    };
    match body_of(mir) {
        Ok(body) => {
            checker.note(signature(mir, body).map(drop));
            for block in &body.blocks {
                if block.unwind.is_some() || block.landing.is_some() {
                    checker.note(Err(Blocker::new(
                        "catch or defer handler",
                        block.id.to_string(),
                    )));
                }
                for statement in &block.statements {
                    checker.note(statement_supported(body, &statement.kind));
                }
                match &block.terminator {
                    Some(terminator) => checker.terminator(body, terminator),
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
        callees: checker.callees,
    }
}

struct Checker<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    blockers: Vec<Blocker>,
    callees: Vec<FunctionLoc<'db>>,
}

impl<'db> Checker<'db> {
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

    fn terminator(&mut self, body: &MirFunctionBody<'db>, terminator: &Terminator<'db>) {
        if let Terminator::Call { callee, .. } = terminator
            && let Ok(function) = direct_callee(self.db, callee)
            && !self.callees.contains(&function)
        {
            self.callees.push(function);
        }
        let outcome = terminator_supported(self.db, body, terminator);
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
        Rvalue::Len(_) => "array length",
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

/// The kind an rvalue evaluates to.
pub(crate) fn rvalue_scalar(
    body: &MirFunctionBody<'_>,
    value: &Rvalue<'_>,
) -> Result<Scalar, Blocker> {
    match value {
        Rvalue::Use(operand) => operand_scalar(body, operand),
        Rvalue::BinaryOp { op, left, right } => {
            let left = operand_scalar(body, left)?;
            let right = operand_scalar(body, right)?;
            binary_typing(*op, left, right).map(|(_, result)| result)
        }
        Rvalue::UnaryOp { op, operand } => unary_typing(*op, operand_scalar(body, operand)?),
        other => Err(Blocker::new(rvalue_name(other), "")),
    }
}

fn statement_supported(
    body: &MirFunctionBody<'_>,
    statement: &StatementKind<'_>,
) -> Result<(), Blocker> {
    match statement {
        StatementKind::Nop => Ok(()),
        StatementKind::Assign { destination, value } => {
            let value = rvalue_scalar(body, value)?;
            let destination = place_scalar(body, destination)?;
            same_kind(destination, value, "assignment")
        }
        // Dropping a scalar does nothing.
        StatementKind::Drop(place) => place_scalar(body, place).map(drop),
        StatementKind::FreshCell { local, .. } => {
            Err(Blocker::new("closure-captured local", local.to_string()))
        }
        StatementKind::Intrinsic { op, .. } => {
            Err(Blocker::new("compiler intrinsic", format!("{op:?}")))
        }
        StatementKind::VirtualFieldStore { .. } => Err(Blocker::new("interface field access", "")),
    }
}

/// A value of one kind is never stored where another is expected: MIR has no
/// implicit conversions between the scalar kinds.
fn same_kind(expected: Scalar, found: Scalar, what: &str) -> Result<(), Blocker> {
    if expected == found {
        Ok(())
    } else {
        Err(Blocker::new(
            format!("{what} between scalar kinds"),
            format!("{found:?} to {expected:?}"),
        ))
    }
}

fn terminator_supported<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &MirFunctionBody<'db>,
    terminator: &Terminator<'db>,
) -> Result<(), Blocker> {
    match terminator {
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => Ok(()),
        Terminator::Branch { condition, .. } => same_kind(
            Scalar::Bool,
            operand_scalar(body, condition)?,
            "branch condition",
        ),
        Terminator::Switch {
            discriminant, arms, ..
        } => {
            if operand_scalar(body, discriminant)? != Scalar::Int {
                return Err(Blocker::new("switch on a non-int value", ""));
            }
            if arms
                .iter()
                .any(|(key, _)| matches!(key, SwitchKey::Class(_)))
            {
                return Err(Blocker::new("switch on a class tag", ""));
            }
            Ok(())
        }
        Terminator::Call {
            has_trace,
            callee,
            args,
            ntypeargs,
            destination,
            unwind,
            ..
        } => {
            let function = direct_callee(db, callee)?;
            let name = || function_display_name(db, DeclRef::Source(function));
            if *has_trace {
                return Err(Blocker::new("call with a trace attachment", name()));
            }
            if *ntypeargs != 0 {
                return Err(Blocker::new("call with type arguments", name()));
            }
            if unwind.is_some() {
                return Err(Blocker::new("catch or defer handler", name()));
            }
            let found = args
                .iter()
                .map(|arg| operand_scalar(body, arg))
                .collect::<Result<Vec<_>, _>>()?;
            let result = place_scalar(body, destination)?;
            // A callee outside the subset is reported as its own blocked
            // function; only a callee that will be compiled constrains the
            // call.
            if let Ok(mir) = lower_function(db, function, OPT)
                && let Ok(callee_body) = body_of(mir)
                && let Ok(expected) = signature(mir, callee_body)
                && (expected.params != found || expected.result != result)
            {
                return Err(Blocker::new(
                    "call that does not match its callee's layout",
                    name(),
                ));
            }
            Ok(())
        }
        Terminator::NarrowBind { .. } => Err(Blocker::new("type test", "narrowing bind")),
        Terminator::VirtualCall { method, .. } => {
            Err(Blocker::new("interface method call", method.clone()))
        }
        Terminator::SysOp { callee, .. } => Err(Blocker::new(
            "system operation",
            match callee {
                Operand::Constant(Constant::Function(function)) => {
                    function_display_name(db, *function)
                }
                _ => String::new(),
            },
        )),
        Terminator::Spawn { .. } => Err(Blocker::new("spawn", "")),
        Terminator::Await { .. } | Terminator::AwaitAny { .. } => Err(Blocker::new("await", "")),
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => Err(Blocker::new("throw", "")),
        Terminator::ShortCircuit {
            operand,
            kind,
            destination,
            ..
        } => {
            if *kind == ShortCircuitKind::Coalesce {
                return Err(Blocker::new("null coalescing", ""));
            }
            same_kind(
                Scalar::Bool,
                operand_scalar(body, operand)?,
                "short-circuit operand",
            )?;
            same_kind(
                place_scalar(body, destination)?,
                Scalar::Bool,
                "short-circuit result",
            )
        }
    }
}
