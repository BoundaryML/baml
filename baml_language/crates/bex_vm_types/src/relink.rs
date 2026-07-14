//! Cross-function index-operand traversal for bytecode relinking.
//!
//! A compiled [`Function`]'s bytecode references the rest of the program
//! through exactly two index spaces: `GlobalIndex` (slots in
//! `Program::globals`) and `ObjectIndex` (slots in the `ObjectPool`).
//! [`visit_index_operands`] visits every such operand — instruction
//! operands, constant-pool entries, and class-init plans — so callers can
//! collect a function's external references or rewrite them when the
//! function is spliced into a program with a different layout.
//!
//! Class *type tags* are deliberately not visited: tags are
//! content-addressed (`baml_type::typetag::class_type_tag`), a pure function
//! of the class's fully-qualified name, so they never change between
//! layouts. Constant-pool / jump-table / init-plan *indices* are
//! function-local and never relocate. Field indices bake a class's layout,
//! which is part of that class's signature — a layout change must recompile
//! the referencing function, not patch it.
//!
//! The instruction match below is EXHAUSTIVE on purpose: adding an opcode
//! that carries a `GlobalIndex`/`ObjectIndex` fails compilation here instead
//! of silently escaping relinking (the failure mode of enumerating only the
//! known-interesting opcodes).

use crate::{
    GlobalIndex, ObjectIndex,
    bytecode::Instruction,
    types::{ConstValue, Function},
};

/// A mutable reference to one cross-function index operand.
pub enum IndexOperand<'a> {
    /// A `Program::globals` slot reference (function or let value).
    Global(&'a mut GlobalIndex),
    /// An `ObjectPool` reference (class/enum/lambda/function object).
    Object(&'a mut ObjectIndex),
}

/// Visit every cross-function index operand in `function`'s bytecode.
pub fn visit_index_operands(function: &mut Function, mut visit: impl FnMut(IndexOperand<'_>)) {
    use Instruction as I;
    for instruction in &mut function.bytecode.instructions {
        match instruction {
            // ── global-slot operands ─────────────────────────────────────
            I::LoadGlobal(slot)
            | I::StoreGlobal(slot)
            | I::SysOp(slot)
            | I::SysOpWithRuntimeId(slot)
            | I::MakeBoundMethod(slot)
            | I::Call { callee: slot, .. }
            | I::CallWithRuntimeId { callee: slot, .. }
            | I::MakeGenericFunction { function: slot, .. } => {
                visit(IndexOperand::Global(slot));
            }
            // ── object-pool operands ─────────────────────────────────────
            I::AllocInstance { class_obj: obj, .. }
            | I::AllocVariant(obj)
            | I::MakeClosure { obj_idx: obj, .. } => {
                visit(IndexOperand::Object(obj));
            }
            // ── no cross-function references ─────────────────────────────
            I::LoadConst(..)
            | I::LoadVar(..)
            | I::StoreVar(..)
            | I::StoreVarLoadVar(..)
            | I::LoadField(..)
            | I::StoreField(..)
            | I::InitField(..)
            | I::InitSpread(..)
            | I::Pop(..)
            | I::Copy(..)
            | I::Jump(..)
            | I::PopJumpIfFalse(..)
            | I::JumpIfFalse(..)
            | I::BinOp(..)
            | I::CmpOp(..)
            | I::AddInt
            | I::SubInt
            | I::MulInt
            | I::DivInt
            | I::ModInt
            | I::AddFloat
            | I::SubFloat
            | I::MulFloat
            | I::DivFloat
            | I::AddBigint
            | I::SubBigint
            | I::MulBigint
            | I::DivBigint
            | I::ModBigint
            | I::BitAndBigint
            | I::BitOrBigint
            | I::BitXorBigint
            | I::ShlBigint
            | I::ShrBigint
            | I::CmpIntOp(..)
            | I::CmpFloatOp(..)
            | I::CmpBigintOp(..)
            | I::UnaryOp(..)
            | I::AllocArray(..)
            | I::AllocMap(..)
            | I::LoadArrayElement
            | I::ContainerLen
            | I::LoadMapElement
            | I::StoreArrayElement
            | I::StoreMapElement
            | I::InitInstance(..)
            | I::Spawn
            | I::Await
            | I::AwaitAny
            | I::CallIndirect
            | I::CallIndirectWithRuntimeId
            | I::VirtualCall { .. }
            | I::VirtualCallWithRuntimeId { .. }
            | I::MakeVirtualBoundMethod { .. }
            | I::Throw
            | I::Rethrow
            | I::Return
            | I::JumpTable(..)
            | I::Discriminant
            | I::TypeTag
            | I::IsType(..)
            | I::LoadType(..)
            | I::DenseTag(..)
            | I::ThrowIfPanic
            | I::Unreachable
            | I::MakeGenericFunctionFromValue { .. }
            | I::MakeCell
            | I::LoadDeref(..)
            | I::StoreDeref(..)
            | I::LoadCapture(..)
            | I::StoreCapture(..)
            | I::CaptureRef(..)
            | I::SendEvent
            | I::LoadVar2(..)
            | I::StoreVar2(..) => {}
        }
    }
    for constant in &mut function.bytecode.constants {
        match constant {
            ConstValue::Object(obj) | ConstValue::ClassWithTypeArgs { class_obj: obj, .. } => {
                visit(IndexOperand::Object(obj));
            }
            _ => {}
        }
    }
    for plan in &mut function.bytecode.class_init_plans {
        visit(IndexOperand::Object(&mut plan.class_obj));
    }
}

/// Visit every cross-function index operand in a pool `object`.
///
/// Compile-time pools contain functions (walked instruction-by-instruction),
/// `GenericFunction` values (whose target is a global slot), and inert
/// literals (strings, bigints, byte arrays) interned during codegen next to
/// the functions that use them. Runtime-only heap shapes never appear in a
/// serialized `Program`; matching them exhaustively keeps this in lockstep
/// with the `Object` enum — a new variant must be classified here before it
/// can slip through a relink.
pub fn visit_object_operands(object: &mut crate::Object, visit: impl FnMut(IndexOperand<'_>)) {
    use crate::Object;
    match object {
        Object::Function(function) => visit_index_operands(function, visit),
        Object::GenericFunction(generic) => {
            let mut visit = visit;
            visit(IndexOperand::Global(&mut generic.function));
        }
        // Inert at relink time: no cross-function index operands.
        Object::Class(..)
        | Object::Enum(..)
        | Object::Interface(..)
        | Object::Package(..)
        | Object::ImplRule(..)
        | Object::String(..)
        | Object::Bigint(..)
        | Object::Uint8Array(..)
        | Object::Type(..) => {}
        // Heap-debug sentinel: never present in a compiled pool.
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(..) => {}
        // Runtime-only heap shapes, unreachable in a compiled pool.
        Object::Instance(..)
        | Object::Variant(..)
        | Object::Closure(..)
        | Object::BoundMethod(..)
        | Object::HostClosure(..)
        | Object::Cell(..)
        | Object::Array(..)
        | Object::Map(..)
        | Object::Float(..)
        | Object::Future(..)
        | Object::UnscheduledFuture(..)
        | Object::RustData(..)
        | Object::Collector(..) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bytecode::{Bytecode, ClassInitPlan},
        types::{FunctionCaptureProps, FunctionKind, FunctionOrigin},
    };

    fn test_function() -> Function {
        use Instruction as I;
        let bytecode = Bytecode {
            instructions: vec![
                I::LoadGlobal(GlobalIndex::from_raw(3)),
                I::Call {
                    callee: GlobalIndex::from_raw(7),
                    ntypeargs: 0,
                },
                I::AllocInstance {
                    class_obj: ObjectIndex::from_raw(2),
                    ntypeargs: 0,
                },
                I::MakeClosure {
                    obj_idx: ObjectIndex::from_raw(9),
                    capture_count: 0,
                    ntypeargs: 0,
                },
                I::LoadConst(0),
                I::Pop(1),
                I::Return,
            ],
            constants: vec![
                ConstValue::Object(ObjectIndex::from_raw(4)),
                ConstValue::Int(5),
                ConstValue::ClassWithTypeArgs {
                    class_obj: ObjectIndex::from_raw(6),
                    type_args_templates: Vec::new(),
                },
            ],
            class_init_plans: vec![ClassInitPlan {
                class_obj: ObjectIndex::from_raw(8),
                ntypeargs: 0,
                fields: Vec::new(),
            }],
            ..Bytecode::default()
        };
        Function {
            name: "test".to_string(),
            source_file: String::new(),
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: baml_type::RuntimeTy::unknown(),
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: None,
            origin: FunctionOrigin::Internal,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
        }
    }

    #[test]
    fn visits_and_rewrites_every_index_operand() {
        let mut function = test_function();

        let mut globals = Vec::new();
        let mut objects = Vec::new();
        visit_index_operands(&mut function, |operand| match operand {
            IndexOperand::Global(slot) => globals.push(slot.raw()),
            IndexOperand::Object(obj) => objects.push(obj.raw()),
        });
        assert_eq!(globals, vec![3, 7], "global-slot operands");
        assert_eq!(objects, vec![2, 9, 4, 6, 8], "object-pool operands");

        // Rewrite through the visitor, then re-collect to prove mutation
        // reaches the stored bytecode (instructions, constants, and plans).
        visit_index_operands(&mut function, |operand| match operand {
            IndexOperand::Global(slot) => *slot = GlobalIndex::from_raw(slot.raw() + 100),
            IndexOperand::Object(obj) => *obj = ObjectIndex::from_raw(obj.raw() + 100),
        });
        let mut rewritten = Vec::new();
        visit_index_operands(&mut function, |operand| match operand {
            IndexOperand::Global(slot) => rewritten.push(slot.raw()),
            IndexOperand::Object(obj) => rewritten.push(obj.raw()),
        });
        assert_eq!(rewritten, vec![103, 107, 102, 109, 104, 106, 108]);
    }
}
