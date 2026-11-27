//! The operators of `baml.ops` the standard library declares in Rust for
//! `int`, `float`, `bool`, `bigint`, `string` and `uint8array`, as
//! functions a row can hold.
//!
//! A call that names its receiver's type compiles such an operator to its
//! own instructions where it stands. Code compiled once for every type
//! finds the operator in the receiver's row instead, as it finds any other
//! method, so each needs a function to be found there: the receiver and
//! the right operand in, each a tag and a payload, the result out.
//!
//! The function is made as MIR and compiled as any other is. It tests that
//! the right operand is of the type the operator is declared for, which
//! the type checker saw to wherever it could name the types; where it is
//! not, `eq` answers `false` and `neq` `true`, and any other operator ends
//! the program.

use std::collections::HashMap;

use baml_compiler2_mir::{
    BasicBlock, BinOp, BlockId, Constant, Local, LocalDecl, MirFunction, MirFunctionBody,
    MirFunctionKind, Operand, Place, RuntimeTy, Rvalue, Statement, StatementKind, Terminator,
    TyTemplate, TypeTest,
};
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{FuncId, Linkage, Module};

use super::{Backend, Declared, Named, Translator, define, internal};
use crate::{
    CompileError,
    check::{Operator, body_of},
    program::{Entry, FnKey, Program},
    types::{Body, NativeTy},
};

/// One operator on a receiver of one type and a right operand of another,
/// and the function a row holds for it.
#[derive(Clone)]
pub(super) struct OperatorCode<'db> {
    operator: Operator,
    receiver: NativeTy<'db>,
    other: NativeTy<'db>,
    id: FuncId,
}

impl<'db> Backend<'db> {
    /// The function of `operator` on a `receiver` and a right operand of
    /// type `other`, declared the first time it is asked for.
    pub(super) fn operator_code(
        &mut self,
        operator: Operator,
        receiver: &NativeTy<'db>,
        other: &NativeTy<'db>,
    ) -> Result<FuncId, CompileError> {
        let known = self.operators.iter().find(|known| {
            known.operator == operator && known.receiver == *receiver && known.other == *other
        });
        if let Some(known) = known {
            return Ok(known.id);
        }
        let name = format!("baml.operator.{}", self.operators.len());
        let signature = Backend::clif_signature(operator.operands(), Entry::Direct, 0, false);
        let id = self
            .module
            .declare_function(&name, Linkage::Local, &signature)
            .map_err(internal)?;
        self.operators.push(OperatorCode {
            operator,
            receiver: receiver.clone(),
            other: other.clone(),
            id,
        });
        Ok(id)
    }
}

/// A type an operator is declared for, as MIR names it for a local and
/// for a test.
fn named(ty: &NativeTy<'_>) -> Result<(RuntimeTy, TyTemplate), CompileError> {
    Ok(match ty {
        NativeTy::Int => (RuntimeTy::Int, TyTemplate::Int),
        NativeTy::Float => (RuntimeTy::Float, TyTemplate::Float),
        NativeTy::Bool => (RuntimeTy::Bool, TyTemplate::Bool),
        NativeTy::String => (RuntimeTy::String, TyTemplate::String),
        NativeTy::Bigint => (RuntimeTy::Bigint, TyTemplate::Bigint),
        NativeTy::Bytes => (RuntimeTy::Uint8Array, TyTemplate::Uint8Array),
        _ => return Err(internal("an operator of the runtime is on a primitive")),
    })
}

fn local(ty: RuntimeTy) -> LocalDecl {
    LocalDecl {
        name: None,
        ty,
        span: None,
        scope_span: None,
        is_captured: false,
    }
}

fn block<'db>(
    id: usize,
    statements: Vec<StatementKind<'db>>,
    terminator: Terminator<'db>,
) -> BasicBlock<'db> {
    BasicBlock {
        id: BlockId(id),
        statements: statements
            .into_iter()
            .map(|kind| Statement { kind, span: None })
            .collect(),
        terminator: Some(terminator),
        span: None,
        terminator_span: None,
        unwind: None,
        handling: None,
        landing: None,
        shielded: false,
        inline: None,
    }
}

fn copy<'db>(local: usize) -> Operand<'db> {
    Operand::Copy(Place::Local(Local(local)))
}

fn assign(value: Rvalue<'_>) -> StatementKind<'_> {
    StatementKind::Assign {
        destination: Place::Local(Local(0)),
        value,
    }
}

/// The body of the function of `code`. Local `_0` is the result, `_1` the
/// receiver and `_2` the right operand as it arrives, of any type; `_3`
/// is that operand once it is known to be of the operator's type.
fn body<'db>(code: &OperatorCode<'db>) -> Result<MirFunctionBody<'db>, CompileError> {
    let (receiver, _) = named(&code.receiver)?;
    let operands = [code.receiver.clone(), code.other.clone()];
    let result = code
        .operator
        .result(&operands[..code.operator.operands()])
        .map_err(internal)?;
    let (result, _) = named(&result)?;
    Ok(match code.operator {
        Operator::Unary(op) => MirFunctionBody {
            blocks: vec![block(
                0,
                vec![assign(Rvalue::UnaryOp {
                    op,
                    operand: copy(1),
                })],
                Terminator::Return,
            )],
            entry: BlockId(0),
            locals: vec![local(result), local(receiver)],
        },
        Operator::Binary(op) => {
            let (other, test) = named(&code.other)?;
            let otherwise = match op {
                BinOp::Eq | BinOp::Ne => block(
                    2,
                    vec![assign(Rvalue::Use(Operand::Constant(Constant::Bool(
                        op == BinOp::Ne,
                    ))))],
                    Terminator::Return,
                ),
                _ => block(2, Vec::new(), Terminator::Unreachable),
            };
            MirFunctionBody {
                blocks: vec![
                    block(
                        0,
                        Vec::new(),
                        Terminator::NarrowBind {
                            source: copy(2),
                            test: TypeTest::Template(test),
                            destination: Local(3),
                            then_block: BlockId(1),
                            else_block: BlockId(2),
                        },
                    ),
                    block(
                        1,
                        vec![assign(Rvalue::BinaryOp {
                            op,
                            left: copy(1),
                            right: copy(3),
                        })],
                        Terminator::Return,
                    ),
                    otherwise,
                ],
                entry: BlockId(0),
                locals: vec![
                    local(result),
                    local(receiver),
                    local(RuntimeTy::Unknown),
                    local(other),
                ],
            }
        }
    })
}

/// Compile the function of every operator a row holds. Each is typed as
/// code of the program's entry function, which like it is entered by name
/// and takes no type objects, and is no frame of a trace: the operator
/// where a call names its receiver's type is none either.
pub(super) fn define_all<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    backend: &mut Backend<'db>,
    program: &Program<'db>,
    declared: &HashMap<FnKey<'db>, Declared<'db>>,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    (clif, disassembly): (&mut Option<String>, &mut Option<String>),
) -> Result<(), CompileError> {
    let entry = program
        .functions
        .first()
        .ok_or_else(|| internal("a program has an entry function"))?;
    let identity = entry
        .mir
        .ok_or_else(|| internal("the entry function has MIR"))?
        .identity
        .clone();
    for code in backend.operators.clone() {
        let mir = MirFunction {
            arity: code.operator.operands(),
            span: None,
            identity: identity.clone(),
            kind: MirFunctionKind::Bytecode(body(&code)?),
            lambdas: Vec::new(),
            signature: None,
        };
        let typed = Body::new(db, &mir, body_of(&mir).map_err(internal)?, &[], &entry.key);
        context.func.signature =
            Backend::clif_signature(code.operator.operands(), Entry::Direct, 0, false);
        Translator::new(
            db,
            backend,
            program,
            declared,
            &typed,
            &mut context.func,
            builder_context,
        )
        .function(&mir, false)?;
        let label = format!(
            "{:?} of {} and {}",
            code.operator,
            crate::types::describe(&code.receiver),
            crate::types::describe(&code.other)
        );
        define(
            backend,
            context,
            code.id,
            &Named::Listed(&label),
            clif,
            disassembly,
        )?;
    }
    Ok(())
}
