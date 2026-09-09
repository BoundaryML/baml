//! Checked registration of host adapter types. The BAML declaration supplies
//! the complete method contract, just as a callable parameter supplies its host
//! closure's contract. Native annotations are not a second runtime type system.

use std::{collections::HashSet, sync::Arc};

use baml_type::{Name, RuntimeTy};
use bex_heap::TlabHolder;
use bex_resource_types::{HostValueArc, HostValueKind};
use bex_vm_types::{
    Class, ClassField, DeclarationName, Object, RealizedTy, TyTemplate, TypeHead, Value,
    types::{Instance, LocalName, MethodImpl, RuntimeImplRule},
};
use indexmap::IndexMap;

use crate::{BexVm, package_baml::ImplResolver, package_load::DynRuleEntry};

/// One explicit implementation, with complete input/associated bindings.
/// The template frame is `[Self]`, so an adapter can implement `I<Self>` or
/// choose `Peer=Self` without publishing a provisional class first.
/// Methods are declared on this interface, not selected by an inherited name.
#[derive(Clone, Debug)]
pub struct HostInterfaceImplementation {
    pub interface: TyTemplate,
    pub methods: Vec<Name>,
}

/// A generated adapter's declaration choice. This contains no host receiver
/// and invokes no user code. Each accepted registration gets a fresh identity.
#[derive(Clone, Debug)]
pub struct HostAdapterDescriptor {
    pub name: Name,
    pub implementations: Vec<HostInterfaceImplementation>,
}

/// The callback at this position in an instance's callback vector implements
/// this exact declaring interface/member. Slots follow declaration order.
#[derive(Clone, Debug)]
pub struct HostCallbackSlot {
    /// Index into the submitted adapter's implementation list. Names alone
    /// cannot distinguish two obligations declaring the same member.
    pub implementation_index: u32,
    pub method: Name,
}

/// VM-local result, not an owning SDK handle. As with other VM allocation APIs,
/// its type must be rooted/exported before releasing the active heap permit.
#[derive(Debug)]
pub struct HostAdapterRegistration {
    pub class: RealizedTy,
    /// Exact realized obligations in descriptor order, including markers and
    /// all-default implementations with no native callback slots.
    pub interfaces: Vec<RealizedTy>,
    pub callbacks: Vec<HostCallbackSlot>,
}

impl BexVm {
    /// Validate and register one immutable adapter type. No callback handles
    /// are acquired here; accepted instances retain their own handles later.
    pub fn register_host_adapter(
        &mut self,
        descriptor: HostAdapterDescriptor,
    ) -> Result<HostAdapterRegistration, String> {
        if descriptor.implementations.is_empty() {
            return Err("host adapter must declare at least one interface".into());
        }
        let mut declarations = Vec::new();
        let mut fields_to_check = Vec::new();
        for implementation in &descriptor.implementations {
            let TyTemplate::Interface(head, args, _, _) = &implementation.interface else {
                return Err("host adapter implementation must name an interface".into());
            };
            if !head.is_resolved() {
                return Err("host adapter interface has not been resolved in this runtime".into());
            }
            let Object::Interface(interface) = self.get_object(head.ptr()) else {
                return Err("host adapter target is not an interface declaration".into());
            };
            if args.len() != interface.args.len() {
                return Err(format!(
                    "interface `{}` expects {} type arguments",
                    interface.name,
                    interface.args.len()
                ));
            }
            fields_to_check.push(head.ptr());
            let mut provided = HashSet::new();
            for name in &implementation.methods {
                if !provided.insert(name) {
                    return Err(format!("duplicate host method `{}.{name}`", interface.name));
                }
                let Some(method) = interface.methods.iter().find(|method| &method.name == name)
                else {
                    return Err(format!(
                        "interface `{}` does not declare method `{name}`",
                        interface.name
                    ));
                };
                if !method.has_receiver {
                    return Err(format!(
                        "host implementation of receiverless method `{}.{name}` is not yet supported",
                        interface.name
                    ));
                }
                if method.generic_params.len() < 1 + args.len()
                    || method.generic_params.len() != method.generic_param_bounds.len()
                    || !matches!(method.signature, TyTemplate::Function { .. })
                {
                    return Err(format!(
                        "interface method `{}.{name}` has incomplete declaration metadata",
                        interface.name
                    ));
                }
            }
            for method in &interface.methods {
                if !provided.contains(&method.name) && method.default_fn.is_null() {
                    return Err(format!(
                        "host adapter is missing required method `{}.{}`",
                        interface.name, method.name
                    ));
                }
            }
            declarations.push(interface.methods.clone());
        }
        // Field requirements are unsupported even if inherited or satisfied by
        // some other rule. Do not expose access to a host object's mutable state.
        let mut visited = HashSet::new();
        while let Some(ptr) = fields_to_check.pop() {
            if !visited.insert(ptr) {
                continue;
            }
            let Object::Interface(interface) = self.get_object(ptr) else {
                return Err("host adapter requirement is not an interface".into());
            };
            if !interface.fields.is_empty() {
                return Err(format!(
                    "host adapter cannot implement field requirements on `{}`",
                    interface.name
                ));
            }
            fields_to_check.extend(
                interface
                    .requires
                    .iter()
                    .map(|required| required.name.ptr()),
            );
        }

        let owner = self.alloc_private_type_owner();
        let tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class = self.tlab_mut().alloc(Object::Class(Box::new(Class {
            name: DeclarationName::Anonymous(descriptor.name.clone()),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag: tag,
            ty_attr: Default::default(),
            has_cleanup: false,
            boundary_projection: baml_type::ClassProjection::Live,
            generic_param_count: 0,
            owner,
        })));
        let concrete = RealizedTy::Class(TypeHead::new(class, tag), vec![], Default::default());
        let mut callbacks = Vec::new();
        let mut interfaces = Vec::new();
        let mut rules = Vec::new();
        for (index, (implementation, methods)) in descriptor
            .implementations
            .iter()
            .zip(declarations)
            .enumerate()
        {
            let implementation_index =
                u32::try_from(index).map_err(|_| "too many host implementations")?;
            let interface = implementation
                .interface
                .substitute(std::slice::from_ref(&concrete), self)
                .map_err(|error| {
                    format!("host adapter interface bindings cannot be realized: {error}")
                })?;
            let RealizedTy::Interface(head, args, pins, _) = &interface else {
                unreachable!()
            };
            interfaces.push(interface.clone());
            let mut provided = IndexMap::new();
            for method in methods
                .iter()
                .filter(|method| implementation.methods.contains(&method.name))
            {
                // Physical slot zero always retains the original host object,
                // including for markers and all-default implementations.
                let slot = callbacks.len() + 1;
                let function = self.alloc_host_method_trampoline(method, slot, owner)?;
                let mut frame = vec![TyTemplate::from(concrete.clone())];
                frame.extend(args.iter().cloned().map(TyTemplate::from));
                provided.insert(
                    method.name.clone(),
                    MethodImpl {
                        fqn: function,
                        frame,
                    },
                );
                callbacks.push(HostCallbackSlot {
                    implementation_index,
                    method: method.name.clone(),
                });
            }
            rules.push(RuntimeImplRule {
                interface_head: head.ptr(),
                for_ty_pattern: concrete.clone().into(),
                generic_param_bounds: vec![],
                interface_args: args.iter().cloned().map(TyTemplate::from).collect(),
                interface_assoc: pins
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.clone().into()))
                    .collect(),
                methods: provided,
                field_links: Box::new([]),
            });
        }
        ImplResolver::new(self)
            .with_staged_rules(&rules)
            .check_staged_interface_constraints()
            .map_err(|error| error.to_string())?;
        // Everything that can reject the descriptor is complete. Link the
        // owning heap graph, then publish the weak lookup index once.
        let Object::Class(declaration) = self.get_object_mut(class) else {
            unreachable!()
        };
        declaration.fields = (0..=callbacks.len())
            .map(|slot| ClassField {
                name: if slot == 0 {
                    "_host_receiver".into()
                } else {
                    format!("_host_callback_{}", slot - 1)
                },
                field_type: RuntimeTy::unknown(),
                field_template: TyTemplate::from(RealizedTy::unknown()),
                description: None,
                alias: None,
                docstring: None,
                other: IndexMap::new(),
                skip: true,
                runtime_type: None,
            })
            .collect();
        let entries: Vec<_> = rules
            .into_iter()
            .map(|rule| {
                let interface = rule.interface_head;
                let rule = self.tlab_mut().alloc(Object::ImplRule(Box::new(rule)));
                (interface, DynRuleEntry { class, rule })
            })
            .collect();
        let Object::Package(package) = self.get_object_mut(owner) else {
            unreachable!()
        };
        package.classes.insert(
            LocalName {
                namespace: vec![],
                name: descriptor.name,
            },
            class,
        );
        package
            .runtime_mut()
            .expect("private owner is runtime-only")
            .host_adapter_callback_count = Some(callbacks.len());
        for (interface, entry) in &entries {
            package
                .impl_rules
                .entry(*interface)
                .or_default()
                .push(entry.rule);
        }
        self.dynamic_dispatch.register_batch(entries);
        Ok(HostAdapterRegistration {
            class: concrete,
            interfaces,
            callbacks,
        })
    }

    /// Instantiate only a registered adapter type with exactly its callable
    /// slots. Validation precedes allocations/retention; no host method runs.
    pub fn create_host_adapter_instance(
        &mut self,
        ty: &RealizedTy,
        receiver: Arc<HostValueArc>,
        callbacks: Vec<Arc<HostValueArc>>,
    ) -> Result<Value, String> {
        let RealizedTy::Class(head, args, _) = ty else {
            return Err("host adapter instance requires its registered class type".into());
        };
        if !head.is_resolved() || !args.is_empty() {
            return Err("invalid host adapter class type".into());
        }
        let Object::Class(class) = self.get_object(head.ptr()) else {
            return Err("host adapter type is not a class".into());
        };
        if class.owner.is_null() {
            return Err("class is not a registered host adapter".into());
        }
        let Object::Package(package) = self.get_object(class.owner) else {
            return Err("host adapter has no owning package".into());
        };
        let count = package
            .runtime()
            .and_then(|runtime| runtime.host_adapter_callback_count)
            .filter(|_| package.classes.values().any(|class| *class == head.ptr()))
            .ok_or("class is not a registered host adapter")?;
        if callbacks.len() != count {
            return Err(format!(
                "host adapter expects {count} callbacks, got {}",
                callbacks.len()
            ));
        }
        if receiver.kind != HostValueKind::Opaque {
            return Err("host adapter receiver must be an opaque host registration".into());
        }
        if callbacks
            .iter()
            .any(|callback| callback.kind != HostValueKind::Callable)
        {
            return Err("host adapter callbacks must all be callable registrations".into());
        }
        let fields = std::iter::once(receiver)
            .chain(callbacks)
            .map(|callback| Value::object(self.tlab_mut().alloc_rust_data(callback)))
            .collect();
        Ok(Value::object(self.tlab_mut().alloc(Object::Instance(
            Instance::new(head.ptr(), Box::new([]), fields),
        ))))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    const SOURCE: &str = r#"
interface Echo<A> {
    type Output
    function echo<T, U>(self, value: T) -> T throws never
    function produce(self, value: A) -> Self.Output throws never
    function default_value(self) -> int throws never { 7 }
}
interface Defaults { function value(self) -> int throws never { 7 } }
function invoke_default(value: Echo<string, Output=int>) -> int { value.default_value() }
interface Field { value: int }
interface InheritedField requires Field {}
interface Static { function make() -> int throws never }
interface Link<T> { type Peer }
interface Root<T> requires Link<T, Peer=Self> {}
interface Gate {}
interface Pick {}
implements<T extends Gate> Pick for T {}
class Ordinary {}
"#;

    fn vm() -> BexVm {
        BexVm::from_program(
            baml_db::testing::compile_source(SOURCE),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    fn interface(
        vm: &BexVm,
        name: &str,
        args: Vec<TyTemplate>,
        pins: Vec<(Name, TyTemplate)>,
        methods: &[&str],
    ) -> HostInterfaceImplementation {
        let head = vm
            .declaration_head(&baml_type::TypeName::from_dotted_path(&format!(
                "user.{name}"
            )))
            .unwrap();
        HostInterfaceImplementation {
            interface: TyTemplate::interface(head, args, pins),
            methods: methods.iter().map(|name| Name::new(name)).collect(),
        }
    }
    fn echo(vm: &BexVm, methods: &[&str]) -> HostInterfaceImplementation {
        interface(
            vm,
            "Echo",
            vec![RealizedTy::string().into()],
            vec![(Name::new("Output"), RealizedTy::int().into())],
            methods,
        )
    }
    fn descriptor(implementations: Vec<HostInterfaceImplementation>) -> HostAdapterDescriptor {
        HostAdapterDescriptor {
            name: Name::new("Adapter"),
            implementations,
        }
    }

    #[test]
    fn host_registration_rejects_incomplete_methods_and_fields_without_publication() {
        let mut vm = vm();
        for (implementation, message) in [
            (echo(&vm, &["echo"]), "missing required method"),
            (
                echo(&vm, &["echo", "produce", "missing"]),
                "does not declare method",
            ),
            (
                echo(&vm, &["echo", "produce", "echo"]),
                "duplicate host method",
            ),
            (
                interface(&vm, "Field", vec![], vec![], &[]),
                "field requirements",
            ),
            (
                interface(&vm, "InheritedField", vec![], vec![], &[]),
                "field requirements",
            ),
            (
                interface(&vm, "Static", vec![], vec![], &["make"]),
                "receiverless method",
            ),
        ] {
            let error = vm
                .register_host_adapter(descriptor(vec![implementation]))
                .unwrap_err();
            assert!(error.contains(message), "{error}");
            assert_eq!(vm.dynamic_dispatch.rule_count(), 0);
        }
    }

    #[test]
    fn host_registration_rejects_bad_pins_and_blanket_overlap_atomically() {
        let mut vm = vm();
        let mut missing_pin = echo(&vm, &["echo", "produce"]);
        let TyTemplate::Interface(_, _, pins, _) = &mut missing_pin.interface else {
            unreachable!()
        };
        pins.clear();
        let bad_frame = interface(
            &vm,
            "Link",
            vec![TyTemplate::TypeArgRef(1)],
            vec![(Name::new("Peer"), RealizedTy::int().into())],
            &[],
        );
        let gate = interface(&vm, "Gate", vec![], vec![], &[]);
        let pick = interface(&vm, "Pick", vec![], vec![], &[]);
        for (rows, message) in [
            (vec![missing_pin], "every associated binding exactly once"),
            (vec![bad_frame], "cannot be realized"),
            (
                vec![gate.clone(), pick.clone()],
                "overlapping implementations",
            ),
            (vec![pick, gate], "overlapping implementations"),
        ] {
            let error = vm.register_host_adapter(descriptor(rows)).unwrap_err();
            assert!(error.contains(message), "{error}");
            assert_eq!(vm.dynamic_dispatch.rule_count(), 0);
        }
    }

    #[test]
    fn host_registration_supports_self_bindings_and_distinct_nominal_types() {
        let mut vm = vm();
        let rows = vec![
            interface(&vm, "Root", vec![TyTemplate::TypeArgRef(0)], vec![], &[]),
            interface(
                &vm,
                "Link",
                vec![TyTemplate::TypeArgRef(0)],
                vec![(Name::new("Peer"), TyTemplate::TypeArgRef(0))],
                &[],
            ),
        ];
        let first = vm.register_host_adapter(descriptor(rows.clone())).unwrap();
        let second = vm.register_host_adapter(descriptor(rows)).unwrap();
        assert_ne!(
            first.class, second.class,
            "equal names and shapes are not equal adapter identities"
        );
        assert!(first.callbacks.is_empty());
        let value = vm
            .create_host_adapter_instance(
                &first.class,
                HostValueArc::new(900, HostValueKind::Opaque),
                vec![],
            )
            .unwrap();
        assert_eq!(
            RealizedTy::from(vm.value_concrete_ty(value).unwrap()),
            first.class
        );
        let root = vm
            .declaration_head(&baml_type::TypeName::from_dotted_path("user.Root"))
            .unwrap();
        assert!(ImplResolver::new(&vm).type_implements(
            &first.class,
            root,
            &[first.class.clone()],
            &[]
        ));
        assert!(!ImplResolver::new(&vm).type_implements(
            &first.class,
            root,
            &[second.class.clone()],
            &[]
        ));
    }

    #[test]
    fn host_instance_checks_type_and_all_callbacks_before_retaining_any() {
        let mut vm = vm();
        let declaration = descriptor(vec![echo(&vm, &["produce", "echo"])]);
        let registered = vm.register_host_adapter(declaration).unwrap();
        assert_eq!(
            registered
                .callbacks
                .iter()
                .map(|slot| slot.method.as_str())
                .collect::<Vec<_>>(),
            ["echo", "produce"]
        );
        let callback = HostValueArc::new(1, HostValueKind::Callable);
        let weak = Arc::downgrade(&callback);
        assert!(
            vm.create_host_adapter_instance(
                &registered.class,
                HostValueArc::new(900, HostValueKind::Opaque),
                vec![callback]
            )
            .unwrap_err()
            .contains("expects 2 callbacks")
        );
        assert!(weak.upgrade().is_none());
        let valid = HostValueArc::new(2, HostValueKind::Callable);
        let weak = Arc::downgrade(&valid);
        let invalid = HostValueArc::new(3, HostValueKind::Opaque);
        assert!(
            vm.create_host_adapter_instance(
                &registered.class,
                HostValueArc::new(900, HostValueKind::Opaque),
                vec![valid, invalid]
            )
            .unwrap_err()
            .contains("must all be callable")
        );
        assert!(
            weak.upgrade().is_none(),
            "reject the whole vector before retaining its first member"
        );
        let ordinary = vm
            .declaration_head(&baml_type::TypeName::from_dotted_path("user.Ordinary"))
            .unwrap();
        let ordinary = RealizedTy::Class(ordinary, vec![], Default::default());
        assert!(
            vm.create_host_adapter_instance(
                &ordinary,
                HostValueArc::new(900, HostValueKind::Opaque),
                vec![]
            )
            .unwrap_err()
            .contains("not a registered host adapter")
        );
    }

    #[test]
    fn a_provided_method_overrides_the_baml_default() {
        let mut vm = vm();
        let declaration = descriptor(vec![echo(&vm, &["default_value", "echo", "produce"])]);
        let registered = vm.register_host_adapter(declaration).unwrap();
        assert_eq!(registered.callbacks[2].method.as_str(), "default_value");
        let value = vm
            .create_host_adapter_instance(
                &registered.class,
                HostValueArc::new(900, HostValueKind::Opaque),
                [1, 2, 3]
                    .into_iter()
                    .map(|key| HostValueArc::new(key, HostValueKind::Callable))
                    .collect(),
            )
            .unwrap();
        let entry = vm.find_function_by_name("user.invoke_default").unwrap();
        vm.set_entry_point(entry, &[value]);
        let args = loop {
            match vm.exec().unwrap() {
                crate::VmExecState::EarlyYield => continue,
                crate::VmExecState::SysOp { args, .. } => break args,
                other => panic!("expected the host override, got {other:?}"),
            }
        };
        let Object::HostClosure(callback) = vm.get_object(args[0].as_object_ptr().unwrap()) else {
            panic!("callback")
        };
        assert_eq!(callback.handle.key, 3);
        vm.stack.push(Value::int(99));
        assert!(
            matches!(vm.exec().unwrap(), crate::VmExecState::Complete(value) if value==Value::int(99))
        );
    }

    #[test]
    #[expect(unsafe_code, reason = "standalone stop-the-world marker receiver test")]
    fn marker_and_default_only_instances_retain_their_original_host_receiver() {
        use bex_vm_types::RootHaver;
        for name in ["Gate", "Defaults"] {
            let mut vm = vm();
            let declaration = descriptor(vec![interface(&vm, name, vec![], vec![], &[])]);
            let registered = vm.register_host_adapter(declaration).unwrap();
            assert!(registered.callbacks.is_empty());
            let receiver = HostValueArc::new(900, HostValueKind::Opaque);
            let weak = Arc::downgrade(&receiver);
            let value = vm
                .create_host_adapter_instance(&registered.class, receiver, vec![])
                .unwrap();
            let heap = Arc::clone(&vm.heap);
            let mut hook = crate::package_load::DynDispatchRoot::new(
                Arc::clone(&vm.dynamic_dispatch),
                Arc::clone(&heap),
            );
            let (_, roots, forwarding) = unsafe {
                heap.collect_garbage_generational(
                    &[value.as_object_ptr().unwrap()],
                    bex_heap::CollectionLevel::Major,
                )
            };
            vm.forward_roots(&forwarding);
            hook.forward_roots(&forwarding);
            assert!(
                weak.upgrade().is_some(),
                "a {name} instance must retain the host object without callbacks"
            );
            let class = vm.as_instance(&Value::object(roots[0])).unwrap().class;
            let (_, _, forwarding) = unsafe {
                heap.collect_garbage_generational(&[class], bex_heap::CollectionLevel::Major)
            };
            vm.forward_roots(&forwarding);
            hook.forward_roots(&forwarding);
            assert!(
                weak.upgrade().is_none(),
                "a retained type must not keep the dead {name} receiver alive"
            );
            assert_eq!(vm.dynamic_dispatch.rule_count(), 1);
        }
    }

    #[test]
    #[expect(
        unsafe_code,
        reason = "standalone stop-the-world registration ownership test"
    )]
    fn retaining_a_host_type_does_not_retain_its_dead_instances() {
        use bex_vm_types::RootHaver;
        let mut vm = vm();
        let declaration = descriptor(vec![echo(&vm, &["echo", "produce"])]);
        let registered = vm.register_host_adapter(declaration).unwrap();
        let callback = HostValueArc::new(1, HostValueKind::Callable);
        let weak = Arc::downgrade(&callback);
        let _instance = vm
            .create_host_adapter_instance(
                &registered.class,
                HostValueArc::new(900, HostValueKind::Opaque),
                vec![callback, HostValueArc::new(2, HostValueKind::Callable)],
            )
            .unwrap();
        let RealizedTy::Class(class, ..) = registered.class else {
            unreachable!()
        };
        let heap = Arc::clone(&vm.heap);
        let mut hook = crate::package_load::DynDispatchRoot::new(
            Arc::clone(&vm.dynamic_dispatch),
            Arc::clone(&heap),
        );
        let (_, roots, forwarding) = unsafe {
            heap.collect_garbage_generational(&[class.ptr()], bex_heap::CollectionLevel::Major)
        };
        vm.forward_roots(&forwarding);
        hook.forward_roots(&forwarding);
        assert!(
            weak.upgrade().is_none(),
            "the retained type must not own an instance's callbacks"
        );
        assert_eq!(
            vm.dynamic_dispatch.rule_count(),
            1,
            "the type still owns its implementation"
        );
        let Object::Class(class) = vm.get_object(roots[0]) else {
            unreachable!()
        };
        let Object::Package(owner) = vm.get_object(class.owner) else {
            unreachable!()
        };
        assert_eq!(
            owner.runtime().unwrap().host_adapter_callback_count,
            Some(2)
        );
    }
}
