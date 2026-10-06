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
//! Type *heads* are visited by their own walk (`head_walk`), since they live
//! inside types rather than in operands; a type switch's declaration keys
//! ([`SwitchKey::Declaration`](crate::bytecode::SwitchKey)) are object
//! operands and ARE visited here. Constant-pool / jump-table / init-plan
//! *indices* are function-local and never relocate. Field indices bake a
//! class's layout, which is part of that class's signature — a layout change
//! must recompile the referencing function, not patch it.
//!
//! The instruction match below is EXHAUSTIVE on purpose: adding an opcode
//! that carries a `GlobalIndex`/`ObjectIndex` fails compilation here instead
//! of silently escaping relinking (the failure mode of enumerating only the
//! known-interesting opcodes).

use crate::{
    GlobalIndex, ObjectIndex,
    bytecode::{ClassInitPlan, Instruction, SwitchDispatch, SwitchKey, SwitchTable},
    types::{ConstValue, Function},
};

/// A mutable reference to one cross-function index operand.
pub enum IndexOperand<'a> {
    /// A `Program::globals` slot reference (function or let value).
    Global(&'a mut GlobalIndex),
    /// An `ObjectPool` reference (class/enum/lambda/function object).
    Object(&'a mut ObjectIndex),
}

/// A shared reference to one cross-function index operand.
pub enum IndexOperandRef<'a> {
    Global(&'a GlobalIndex),
    Object(&'a ObjectIndex),
}

// Keep the exhaustive operand classification — instructions, constant pool, and
// class-init plans — in one place for the mutable relinker and the read-only
// dependency collector. Every match here is exhaustive on purpose: a new
// operand-carrying opcode or `ConstValue` variant fails compilation instead of
// silently escaping the walk.
macro_rules! visit_bytecode_index_operands {
    ($instructions:expr, $constants:expr, $plans:expr, $tables:expr, $visit:ident, $operand:ident) => {{
        use Instruction as I;
        let mut bakes_type_layout = false;
        for instruction in $instructions {
            match instruction {
            // ── global-slot operands ─────────────────────────────────────
            I::LoadGlobal(slot)
            | I::StoreGlobal(slot)
            | I::SysOp(slot)
            | I::MakeBoundMethod(slot)
            | I::CallHooked { callee: slot, .. }
            | I::Call { callee: slot, .. }
            | I::MakeGenericFunction { function: slot, .. } => {
                $visit($operand::Global(slot));
            }
            // ── object-pool operands ─────────────────────────────────────
            I::AllocInstance { class_obj: obj, .. }
            | I::AllocVariant(obj)
            | I::MakeClosure { obj_idx: obj, .. } => {
                $visit($operand::Object(obj));
            }
            // These operands bake field/discriminant/type/vtable layout without
            // retaining the referenced type's identity. The read-only caller
            // uses this bit to add its conservative layout dependency.
            I::LoadField(..)
            | I::StoreField(..)
            | I::VirtualLoadField(..)
            | I::VirtualStoreField(..)
            | I::InitField(..)
            | I::InitSpread(..)
            | I::InitInstance(..)
            | I::VirtualCall { .. }
            | I::MakeVirtualBoundMethod { .. }
            | I::MakeVirtualFunction { .. }
            | I::JumpTable(..)
            | I::Discriminant
            | I::TypeTag
            | I::IsType(..)
            | I::NarrowBind { .. }
            | I::LoadType(..)
            | I::BindType(..)
            | I::DenseTag(..) => bakes_type_layout = true,
            // ── no cross-function references ─────────────────────────────
            I::LoadConst(..)
            | I::LoadCurrentPackage(..)
            | I::LoadVar(..)
            | I::StoreVar(..)
            | I::StoreVarLoadVar(..)
            | I::Pop(..)
            | I::Copy(..)
            | I::Jump(..)
            | I::PopJumpIfFalse(..)
            | I::JumpIfFalse(..)
            | I::PopJumpIfTrue(..)
            | I::JumpIfFalseOrPop(..)
            | I::JumpIfTrueOrPop(..)
            | I::JumpIfNotNullOrPop(..)
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
            | I::Spawn
            | I::Await
            | I::AwaitAny
            | I::CallIndirect
            | I::BeginTraceHook(_)
            | I::EndTraceHook
        | I::TraceHookHidden
        | I::TraceHookTiming
        | I::TraceHookSpan
        | I::TraceHookRich
            | I::SetCallTrace
            | I::Throw
            | I::Rethrow
            | I::Return
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
        // Constant-pool object references. Exhaustive on `ConstValue` so a new
        // object-carrying variant fails compilation here rather than escaping.
        for constant in $constants {
            match constant {
                ConstValue::Object(obj)
                | ConstValue::ClassWithTypeArgs { class_obj: obj, .. } => {
                    $visit($operand::Object(obj));
                }
                ConstValue::OmittedArg
                | ConstValue::Null
                | ConstValue::Int(_)
                | ConstValue::Float(_)
                | ConstValue::Bool(_)
                | ConstValue::Type(_)
                | ConstValue::Literal(_) => {}
            }
        }
        // Each class-init plan carries one class-object reference.
        for plan in $plans {
            let ClassInitPlan { class_obj, .. } = plan;
            $visit($operand::Object(class_obj));
        }
        // A switch's declaration keys name their declarations by object
        // operand; the kind keys are constants. A solved table holds values,
        // which name nothing.
        for table in $tables {
            let SwitchTable { dispatch, .. } = table;
            match dispatch {
                SwitchDispatch::Keys(keys) => {
                    for key in keys {
                        match key {
                            SwitchKey::Declaration(declaration) => {
                                $visit($operand::Object(declaration));
                            }
                            SwitchKey::Kind(_) => {}
                        }
                    }
                }
                SwitchDispatch::Hash { .. } | SwitchDispatch::Sorted(_) => {}
            }
        }
        bakes_type_layout
    }};
}

/// Visit every cross-function index operand in `function`'s bytecode.
pub fn visit_index_operands(function: &mut Function, mut visit: impl FnMut(IndexOperand<'_>)) {
    let _ = visit_bytecode_index_operands!(
        &mut function.bytecode.instructions,
        &mut function.bytecode.constants,
        &mut function.bytecode.class_init_plans,
        &mut function.bytecode.switch_tables,
        visit,
        IndexOperand
    );
}

/// Read every cross-function index operand without cloning the function.
/// Returns whether an instruction also bakes unnamed type-layout information.
pub fn visit_index_operands_ref(
    function: &Function,
    mut visit: impl FnMut(IndexOperandRef<'_>),
) -> bool {
    visit_bytecode_index_operands!(
        &function.bytecode.instructions,
        &function.bytecode.constants,
        &function.bytecode.class_init_plans,
        &function.bytecode.switch_tables,
        visit,
        IndexOperandRef
    )
}

// The mutable relocator and read-only dependency collector share one
// exhaustive object classification, just as they share the bytecode walker.
macro_rules! visit_object_index_operands {
    ($doc:literal, $name:ident, $function_walk:ident, $operand:ident, $values:ident $(, $mutable:tt)?) => {
        #[doc = $doc]
        pub fn $name(object: &$($mutable)? crate::Object, visit: impl FnMut($operand<'_>)) {
            use crate::Object;
            match object {
                Object::Function(function) => { let _ = $function_walk(function, visit); }
                Object::GenericFunction(generic) => {
                    let mut visit = visit;
                    visit($operand::Global(&$($mutable)? generic.function));
                }
                // Defaults and inherent methods name pooled functions.
                Object::Interface(interface) => {
                    let mut visit = visit;
                    if let Some(default) = &$($mutable)? interface.structural_default {
                        visit($operand::Object(&$($mutable)? default.function));
                    }
                    for method in &$($mutable)? interface.methods {
                        if let Some(default) = &$($mutable)? method.default {
                            visit($operand::Object(default));
                        }
                    }
                }
                Object::Class(class) => {
                    let mut visit = visit;
                    for method in class.methods.$values() {
                        visit($operand::Object(&$($mutable)? method.function));
                    }
                }
                // Inert at relink time: no cross-object index operands.
                Object::Enum(..) | Object::TypeAlias(..) | Object::Package(..)
                | Object::ImplRule(..) | Object::String(..) | Object::Bigint(..)
                | Object::Uint8Array(..) | Object::Type(..) => {}
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(..) => {}
                // Runtime-only heap shapes, absent from serialized programs.
                Object::Instance(..) | Object::Variant(..) | Object::Closure(..)
                | Object::BoundMethod(..) | Object::HostClosure(..) | Object::Cell(..)
                | Object::Array(..) | Object::Map(..) | Object::Float(..)
                | Object::Future(..) | Object::RustData(..) | Object::Tombstone => {}
            }
        }
    };
}

visit_object_index_operands!(
    "Visit every cross-object index operand, including class methods and interface defaults.",
    visit_object_operands,
    visit_index_operands,
    IndexOperand,
    values_mut,
    mut
);
visit_object_index_operands!(
    "Read every cross-object index operand without cloning the executable object.",
    visit_object_operands_ref,
    visit_index_operands_ref,
    IndexOperandRef,
    values
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HeapPtr, Object,
        bytecode::{Bytecode, ClassInitPlan},
        types::{FunctionKind, FunctionOrigin},
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
            switch_tables: vec![SwitchTable::of_keys(
                vec![
                    SwitchKey::Kind(0),
                    SwitchKey::Declaration(ObjectIndex::from_raw(11)),
                ],
                vec!["int".to_string(), "Widget".to_string()],
            )],
            ..Bytecode::default()
        };
        Function {
            name: "test".to_string(),
            source_file: String::new(),
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            telemetry_function_id: None,
            telemetry_registration: crate::FunctionRegistration::default(),
            telemetry_policy_id: crate::TelemetryPolicyId::none(),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: crate::TyTemplate::Unknown,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: crate::TyTemplate::Never,
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,
            runtime_package: HeapPtr::null(),
        }
    }

    /// A function's telemetry policy is runtime state: it is no part of the
    /// artifact, and a loaded function starts with none.
    #[test]
    fn function_artifacts_exclude_runtime_policy_and_load_with_none() {
        let object = Object::Function(Box::new(test_function()));
        let original_bytes = borsh::to_vec(&object).unwrap();
        let Object::Function(function) = &object else {
            unreachable!()
        };
        function.telemetry_policy_id.store(17);
        let bytes = borsh::to_vec(&object).unwrap();
        assert_eq!(bytes, original_bytes);

        let Object::Function(loaded) = borsh::from_slice::<Object>(&bytes).unwrap() else {
            unreachable!()
        };
        assert_eq!(
            loaded.telemetry_policy_id.load(),
            crate::TelemetryPolicyId::NONE
        );
    }

    #[test]
    fn hooked_calls_share_read_only_and_mutable_global_edges() {
        let mut function = test_function();
        function.bytecode.instructions = vec![Instruction::CallHooked {
            callee: GlobalIndex::from_raw(31),
            ntypeargs: 0,
        }];
        let mut globals = Vec::new();
        visit_index_operands_ref(&function, |operand| {
            if let IndexOperandRef::Global(slot) = operand {
                globals.push(slot.raw());
            }
        });
        assert_eq!(globals, vec![31]);
        visit_index_operands(&mut function, |operand| {
            if let IndexOperand::Global(slot) = operand {
                *slot = GlobalIndex::from_raw(7);
            }
        });
        assert!(matches!(
            function.bytecode.instructions[0],
            Instruction::CallHooked { callee, .. } if callee.raw() == 7
        ));
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
        assert_eq!(objects, vec![2, 9, 4, 6, 8, 11], "object-pool operands");

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
        assert_eq!(rewritten, vec![103, 107, 102, 109, 104, 106, 108, 111]);
    }
}
