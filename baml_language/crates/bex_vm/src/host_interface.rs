//! Ordinary VM function bodies for provided host-interface methods.
//!
//! Registration/conformance is a separate gate. These helpers are the executable
//! part of an already checked adapter: one immutable function per method, with
//! each instance retaining its own host callback in a hidden field.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    Bytecode, ConstValue, Function, FunctionCaptureProps, HeapPtr, HostClosure, Instruction,
    Object, TyTemplate, Value,
    types::{FunctionKind, FunctionOrigin, InterfaceMethodDef},
};

use crate::{
    BexVm,
    package_baml::{NativeCallResult, NativeFunction},
};

impl BexVm {
    /// Retain the callback for one provided method on one host instance. The
    /// callback's captured receiver stays alive through HostValueArc, exactly
    /// like an ordinary host callable. No runtime-global strong cache is added.
    pub fn alloc_host_method_callback(
        &mut self,
        callback: Arc<bex_resource_types::HostValueArc>,
    ) -> Result<Value, String> {
        if callback.kind != bex_resource_types::HostValueKind::Callable {
            return Err("host method requires a callable registration".into());
        }
        Ok(Value::object(self.tlab_mut().alloc_rust_data(callback)))
    }

    /// Build a method using the interface declaration's full frame:
    /// `[Self, interface arguments..., method arguments...]`. The implementation
    /// row must supply the Self/interface prefix; invocation appends the method
    /// arguments, including unused ones retained in the declaration metadata.
    ///
    /// `callback_slot` is a hidden physical field on the synthetic adapter class.
    /// The caller owns validating the class layout and method conformance before
    /// publishing the rule. This function neither registers a type nor exposes
    /// an unchecked host object as implementing an interface.
    pub fn alloc_host_method_trampoline(
        &mut self,
        method: &InterfaceMethodDef,
        callback_slot: usize,
        owner: HeapPtr,
    ) -> Result<HeapPtr, String> {
        if owner.is_null()
            || !matches!(self.get_object(owner), Object::Package(package)
                if matches!(package.kind, bex_vm_types::types::PackageKind::Runtime(_)))
        {
            return Err("host method requires an owning runtime package".into());
        }
        if !method.has_receiver {
            return Err("instance host trampoline requires a receiver method".into());
        }
        if method.generic_params.is_empty()
            || method.generic_params.len() != method.generic_param_bounds.len()
        {
            return Err("host method has an incomplete generic frame".into());
        }
        let TyTemplate::Function {
            params,
            ret,
            throws,
            ..
        } = &method.signature
        else {
            return Err("host method declaration has no callable signature".into());
        };

        // The helper takes a retained raw callback and a fully realized callable
        // type. It returns a fresh, monomorphic HostClosure for this invocation;
        // sharing one bound closure across different T choices would be wrong.
        let helper_signature = TyTemplate::Function {
            params: ["callback", "signature"]
                .into_iter()
                .map(|name| baml_type::TyTemplateFunctionParamTy {
                    name: Some(baml_type::Name::new(name)),
                    ty: TyTemplate::Unknown {
                        attr: Default::default(),
                    },
                    mode: baml_type::FunctionParamMode::Required,
                })
                .collect(),
            ret: Box::new(TyTemplate::Unknown {
                attr: Default::default(),
            }),
            throws: Box::new(TyTemplate::Never {
                attr: Default::default(),
            }),
            attr: Default::default(),
        };
        let mut helper = make_function(
            "$host_method::bind",
            helper_signature,
            Bytecode::new(),
            FunctionKind::Native(bind_host_method as NativeFunction as *const ()),
        );
        helper.runtime_package = owner;
        let helper_ptr = self.tlab_mut().alloc(Object::Function(Box::new(helper)));

        let mut bytecode = Bytecode::new();
        // Runtime-only pointer constants live in resolved_constants and are
        // traced/forwarded with the owning Function. The Type template itself
        // remains in constants for LoadType's ordinary frame substitution.
        bytecode.constants = vec![ConstValue::Type(method.signature.clone()), ConstValue::Null];
        bytecode.resolved_constants = vec![Value::NULL, Value::object(helper_ptr)];
        for index in 0..params.len() {
            bytecode.instructions.push(Instruction::LoadVar(index + 2));
        }
        bytecode.instructions.extend([
            Instruction::LoadVar(1),
            Instruction::LoadField(callback_slot),
            Instruction::LoadType(0),
            Instruction::LoadConst(1),
        ]);
        bytecode.call_layouts.insert(
            bytecode.instructions.len(),
            baml_type::CallLayout::positional(2),
        );
        bytecode.instructions.push(Instruction::CallIndirect);
        bytecode.call_layouts.insert(
            bytecode.instructions.len(),
            baml_type::CallLayout::from_modes(params.iter().map(|p| (p.name.clone(), p.mode))),
        );
        bytecode
            .instructions
            .extend([Instruction::CallIndirect, Instruction::Return]);
        bytecode.compact = Some(bytecode.lower_to_compact());

        let mut with_receiver = vec![baml_type::TyTemplateFunctionParamTy {
            name: Some(baml_type::Name::new("self")),
            ty: TyTemplate::TypeArgRef(0),
            mode: baml_type::FunctionParamMode::Required,
        }];
        with_receiver.extend(params.iter().cloned());
        let signature = TyTemplate::Function {
            params: with_receiver,
            ret: ret.clone(),
            throws: throws.clone(),
            attr: Default::default(),
        };
        let mut function = make_function(
            &format!("$host_method::{}", method.name),
            signature,
            bytecode,
            FunctionKind::Bytecode,
        );
        function.runtime_package = owner;
        function.declared_name = Some(method.name.to_string());
        function.is_interface_body = true;
        function.display_type_params = method
            .generic_params
            .iter()
            .map(ToString::to_string)
            .collect();
        function.generic_param_bounds = method.generic_param_bounds.clone();
        Ok(self.tlab_mut().alloc(Object::Function(Box::new(function))))
    }
}

fn make_function(
    name: &str,
    signature: TyTemplate,
    bytecode: Bytecode,
    kind: FunctionKind,
) -> Function {
    let TyTemplate::Function {
        params,
        ret,
        throws,
        ..
    } = signature
    else {
        unreachable!()
    };
    Function {
        name: name.into(),
        source_file: String::new(),
        docstring: None,
        declared_name: None,
        arity: params.len(),
        real_local_count: 0,
        bytecode,
        kind,
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: baml_type::Span::fake(),
        display_return_type: ret.to_string(),
        return_type: *ret,
        param_names: params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                p.name
                    .as_ref()
                    .map_or_else(|| format!("arg{i}"), ToString::to_string)
            })
            .collect(),
        param_types: params.iter().map(|p| p.ty.clone()).collect(),
        param_has_default: params.iter().map(|p| p.is_optional()).collect(),
        display_type_params: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: params.iter().map(|p| p.ty.to_string()).collect(),
        throws_type: *throws,
        origin: FunctionOrigin::Internal,
        is_interface_body: false,
        native_key: None,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0,
        runtime_package: HeapPtr::null(),
    }
}

fn bind_host_method(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
    match bind_host_method_inner(vm, args) {
        Ok(value) => NativeCallResult::Done(value),
        Err(message) => bex_vm_types::errors::VmInternalError::BridgeFailure { message }.into(),
    }
}

fn bind_host_method_inner(vm: &mut BexVm, args: &[Value]) -> Result<Value, String> {
    let [callback, signature] = args else {
        return Err("host method binding expects callback and signature".into());
    };
    let callback = callback
        .as_object_ptr()
        .ok_or("host method callback is not retained")?;
    let Object::RustData(callback) = vm.get_object(callback) else {
        return Err("host method callback has an invalid representation".into());
    };
    let callback = Arc::clone(callback)
        .downcast::<bex_resource_types::HostValueArc>()
        .map_err(|_| "host method callback has an invalid registration")?;
    if callback.kind != bex_resource_types::HostValueKind::Callable {
        return Err("host method callback is not callable".into());
    }
    let signature = signature
        .as_object_ptr()
        .ok_or("host method signature is not a type")?;
    let Object::Type(signature) = vm.get_object(signature) else {
        return Err("host method signature is not a type".into());
    };
    let bex_vm_types::RealizedTy::Function {
        params,
        ret,
        throws,
        ..
    } = &signature.ty
    else {
        return Err("host method signature is not callable".into());
    };
    let closure = HostClosure {
        handle: callback,
        ret_ty: ret.clone(),
        throws_ty: throws.clone(),
        arity: params.len(),
        params: Box::new(params.clone()),
    };
    Ok(Value::object(
        vm.tlab_mut().alloc(Object::HostClosure(closure)),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use bex_resource_types::{HostValueArc, HostValueKind};
    use bex_vm_types::{RealizedTy, TypeHead};

    use super::*;
    use crate::{HostAdapterDescriptor, HostInterfaceImplementation, VmExecState};

    const SOURCE: &str = r#"
interface Echo<A> {
    type Output
    function echo<T, U>(self, value: T) -> T throws never
    function produce(self, value: A) -> Self.Output throws never
    function again<T>(self, value: T) -> T throws never { self.echo<T, int>(value) }
}
function exercise(value: Echo<string, Output=int>) -> int {
    assert.equal(value.echo<string, int>("Ada"), "Ada");
    assert.equal(value.echo<int, string>(7), 7);
    assert.equal(value.again<string>("default"), "default");
    let bound = value.echo<string, int>;
    assert.equal(bound("bound"), "bound");
    value.produce("answer")
}
function pair(left: Echo<string, Output=int>, right: Echo<string, Output=int>) -> int {
    left.produce("left") + right.produce("right")
}
"#;

    fn head(vm: &BexVm, name: &str) -> TypeHead {
        vm.declaration_head(&baml_type::TypeName::from_dotted_path(&format!(
            "user.{name}"
        )))
        .unwrap()
    }

    // The test observes the exact VM→engine sysop, including realized contracts
    // and omitted optional slots. This cannot be tested in a BAML test block.
    fn setup() -> (BexVm, HeapPtr) {
        let mut vm = BexVm::from_program(
            baml_db::testing::compile_source(SOURCE),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let interface = head(&vm, "Echo");
        let registered = vm
            .register_host_adapter(HostAdapterDescriptor {
                name: baml_type::Name::new("HostAdapter"),
                implementations: vec![HostInterfaceImplementation {
                    interface: RealizedTy::Interface(
                        interface,
                        vec![RealizedTy::string()],
                        vec![(baml_type::Name::new("Output"), RealizedTy::int())],
                        Default::default(),
                    )
                    .into(),
                    methods: vec![
                        baml_type::Name::new("echo"),
                        baml_type::Name::new("produce"),
                    ],
                }],
            })
            .unwrap();
        let RealizedTy::Class(class, ..) = registered.class else {
            unreachable!()
        };
        let class_ptr = class.ptr();
        (vm, class_ptr)
    }

    fn instance(vm: &mut BexVm, class: HeapPtr, echo_key: u64, produce_key: u64) -> Value {
        let Object::Class(declaration) = vm.get_object(class) else {
            unreachable!()
        };
        let ty = RealizedTy::Class(
            TypeHead::new(class, declaration.type_tag),
            vec![],
            Default::default(),
        );
        vm.create_host_adapter_instance(
            &ty,
            HostValueArc::new(900, HostValueKind::Opaque),
            vec![
                HostValueArc::new(echo_key, HostValueKind::Callable),
                HostValueArc::new(produce_key, HostValueKind::Callable),
            ],
        )
        .unwrap()
    }

    #[test]
    fn host_trampolines_realize_each_generic_call_and_preserve_defaults_and_bound_methods() {
        let (mut vm, class) = setup();
        let receiver = instance(&mut vm, class, 1, 2);
        let entry = vm.find_function_by_name("user.exercise").unwrap();
        vm.set_entry_point(entry, &[receiver]);
        let mut calls = 0;
        let result = loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => continue,
                VmExecState::Complete(result) => break result,
                VmExecState::SysOp { operation, args } => {
                    assert_eq!(operation, bex_vm_types::SysOp::BamlHostCallHostValue);
                    let Object::HostClosure(callback) =
                        vm.get_object(args[0].as_object_ptr().unwrap())
                    else {
                        panic!("not a checked host closure")
                    };
                    let key = callback.handle.key;
                    assert_eq!(*callback.throws_ty, RealizedTy::never());
                    let packed = vm.as_array(&args[1]).unwrap().to_vec();
                    let positional = vm.as_array(&packed[0]).unwrap().to_vec();
                    let Object::Map(optional) = vm.get_object(packed[1].as_object_ptr().unwrap())
                    else {
                        unreachable!()
                    };
                    let result = if key == 1 {
                        assert_eq!(callback.arity, 1);
                        if calls == 1 {
                            assert_eq!(*callback.ret_ty, RealizedTy::int());
                            assert_eq!(callback.params[0].ty, RealizedTy::int());
                            assert!(optional.lock().is_empty());
                        } else {
                            assert_eq!(*callback.ret_ty, RealizedTy::string());
                            assert_eq!(callback.params[0].ty, RealizedTy::string());
                            assert!(optional.lock().is_empty());
                        }
                        positional[0]
                    } else {
                        assert_eq!(key, 2);
                        assert_eq!(*callback.ret_ty, RealizedTy::int());
                        assert_eq!(callback.params[0].ty, RealizedTy::string());
                        Value::int(73)
                    };
                    calls += 1;
                    vm.stack.push(result);
                }
                other => panic!("unexpected VM state: {other:?}"),
            }
        };
        assert_eq!(calls, 5);
        assert_eq!(result, Value::int(73));
    }

    #[test]
    fn one_host_adapter_type_retains_independent_instance_callbacks() {
        let (mut vm, class) = setup();
        let left = instance(&mut vm, class, 11, 21);
        let right = instance(&mut vm, class, 12, 22);
        let entry = vm.find_function_by_name("user.pair").unwrap();
        vm.set_entry_point(entry, &[left, right]);
        let mut called = Vec::new();
        let result = loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => continue,
                VmExecState::Complete(result) => break result,
                VmExecState::SysOp { args, .. } => {
                    let Object::HostClosure(callback) =
                        vm.get_object(args[0].as_object_ptr().unwrap())
                    else {
                        unreachable!()
                    };
                    let key = callback.handle.key;
                    called.push(key);
                    vm.stack.push(Value::int(key as i64));
                }
                other => panic!("unexpected VM state: {other:?}"),
            }
        };
        assert_eq!(called, vec![21, 22]);
        assert_eq!(result, Value::int(43));
    }
    #[test]
    fn host_trampoline_preserves_supplied_and_omitted_optional_slots() {
        let (mut vm, class) = setup();
        let receiver = instance(&mut vm, class, 1, 2);
        let Object::Class(declaration) = vm.get_object(class) else {
            unreachable!()
        };
        let owner = declaration.owner;
        let Object::Interface(interface) = vm.get_object(head(&vm, "Echo").ptr()) else {
            unreachable!()
        };
        let mut method = interface
            .methods
            .iter()
            .find(|m| m.name.as_str() == "echo")
            .unwrap()
            .clone();
        // The current source grammar rejects optional interface declarations.
        // Exercise the already-defined runtime calling mode directly; this is
        // not evidence that that source-language/API gap has been implemented.
        let TyTemplate::Function { params, .. } = &mut method.signature else {
            unreachable!()
        };
        params.push(baml_type::TyTemplateFunctionParamTy::optional(
            Some(baml_type::Name::new("flag")),
            TyTemplate::Bool {
                attr: Default::default(),
            },
        ));
        let method = vm.alloc_host_method_trampoline(&method, 1, owner).unwrap();
        let concrete: RealizedTy = vm.value_concrete_ty(receiver).unwrap().into();
        for supplied in [Value::OMITTED_ARG, Value::bool(true)] {
            vm.set_entry_point_with_type_args(
                method,
                &[receiver, Value::int(7), supplied],
                [
                    ("Self".into(), concrete.clone()),
                    ("A".into(), RealizedTy::string()),
                    ("T".into(), RealizedTy::int()),
                    ("U".into(), RealizedTy::string()),
                ]
                .into_iter()
                .collect(),
            );
            let args = loop {
                match vm.exec().unwrap() {
                    VmExecState::EarlyYield => continue,
                    VmExecState::SysOp { args, .. } => break args,
                    other => panic!("unexpected state: {other:?}"),
                }
            };
            let packed = vm.as_array(&args[1]).unwrap().to_vec();
            assert_eq!(
                vm.as_array(&packed[0]).unwrap().as_slice(),
                &[Value::int(7)]
            );
            let Object::Map(optional) = vm.get_object(packed[1].as_object_ptr().unwrap()) else {
                unreachable!()
            };
            if supplied.is_omitted() {
                assert!(optional.lock().is_empty());
            } else {
                let values = optional.lock();
                assert_eq!(values.len(), 1);
                let (name, value) = values.first().unwrap();
                assert_eq!(name.as_str(), "flag");
                assert_eq!(*value, Value::bool(true));
            }
            vm.stack.push(Value::int(7));
            assert!(matches!(vm.exec().unwrap(),VmExecState::Complete(v) if v==Value::int(7)));
        }
    }

    #[test]
    #[expect(unsafe_code, reason = "standalone stop-the-world VM ownership test")]
    fn host_adapter_methods_and_callbacks_survive_gc_then_release_without_global_roots() {
        use bex_vm_types::RootHaver;
        let (mut vm, class) = setup();
        let callback = HostValueArc::new(101, HostValueKind::Callable);
        let weak = Arc::downgrade(&callback);
        let Object::Class(declaration) = vm.get_object(class) else {
            unreachable!()
        };
        let ty = RealizedTy::Class(
            TypeHead::new(class, declaration.type_tag),
            vec![],
            Default::default(),
        );
        let receiver = vm
            .create_host_adapter_instance(
                &ty,
                HostValueArc::new(900, HostValueKind::Opaque),
                vec![callback, HostValueArc::new(102, HostValueKind::Callable)],
            )
            .unwrap();
        let entry = vm.find_function_by_name("user.exercise").unwrap();
        vm.set_entry_point(entry, &[receiver]);
        let heap = Arc::clone(&vm.heap);
        let mut hook = crate::package_load::DynDispatchRoot::new(
            Arc::clone(&vm.dynamic_dispatch),
            Arc::clone(&heap),
        );
        let rule = vm.dynamic_dispatch.rules_for_class(class)[0];
        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            !roots.contains(&rule),
            "the test must not artificially root the implementation rule"
        );
        let (_, _, forwarding) =
            unsafe { heap.collect_garbage_generational(&roots, bex_heap::CollectionLevel::Major) };
        assert!(
            forwarding.contains_key(&rule),
            "the receiver's class must own its rules"
        );
        vm.forward_roots(&forwarding);
        hook.forward_roots(&forwarding);
        assert!(weak.upgrade().is_some());
        let mut calls = 0;
        loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => continue,
                VmExecState::Complete(value) => {
                    assert_eq!(value, Value::int(73));
                    break;
                }
                VmExecState::SysOp { mut args, .. } => {
                    if calls == 0 {
                        let mut roots = Vec::new();
                        vm.collect_roots(&mut roots);
                        roots.extend(args.iter().filter_map(Value::as_object_ptr));
                        let (_, _, forwarding) = unsafe {
                            heap.collect_garbage_generational(
                                &roots,
                                bex_heap::CollectionLevel::Major,
                            )
                        };
                        vm.forward_roots(&forwarding);
                        hook.forward_roots(&forwarding);
                        for value in &mut args {
                            if let Some(ptr) = value.as_object_ptr() {
                                if let Some(moved) = forwarding.get(&ptr) {
                                    *value = Value::object(*moved);
                                }
                            }
                        }
                    }
                    let Object::HostClosure(callback) =
                        vm.get_object(args[0].as_object_ptr().unwrap())
                    else {
                        unreachable!()
                    };
                    let value = if callback.handle.key == 101 {
                        let packed = vm.as_array(&args[1]).unwrap();
                        vm.as_array(&packed[0]).unwrap()[0]
                    } else {
                        Value::int(73)
                    };
                    calls += 1;
                    vm.stack.push(value);
                }
                other => panic!("unexpected state: {other:?}"),
            }
        }
        assert_eq!(calls, 5);
        drop(vm);
        let (_, _, forwarding) =
            unsafe { heap.collect_garbage_generational(&[], bex_heap::CollectionLevel::Major) };
        hook.forward_roots(&forwarding);
        assert!(
            weak.upgrade().is_none(),
            "no global registration/cache may retain the host receiver"
        );
        assert_eq!(hook.tables.rule_count(), 0);
    }
}
