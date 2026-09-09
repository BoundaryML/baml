//! Owning engine boundary for checked host adapter registration.

use std::sync::Arc;

use baml_type::{Name, TyTemplate, TypeName};
use bex_external_types::Handle;
use bex_heap::{HeapPermit, TlabHolder};
use bex_resource_types::HostValueArc;
use bex_vm_types::{Object, RealizedTy, TypeHead, ValueKind, types::TypeValue};

use crate::{BexEngine, CancellationToken, EngineError};

/// A declaration selected from the loaded program or retained by this engine.
/// A retained handle identifies a declaration object, not a reflected Type
/// wrapper. It is validated under the receiving engine's heap permit.
#[derive(Clone, Debug)]
pub enum HostDeclaration {
    Named(TypeName),
    Retained(Handle),
}

#[derive(Clone, Debug)]
pub struct HostAdapterImplementation {
    /// Slot zero is the new concrete Self. The explicit-evidence registration
    /// entry point also binds slots 1.. from its supplied type arguments.
    pub interface: TyTemplate<HostDeclaration>,
    pub methods: Vec<Name>,
}

#[derive(Clone, Debug)]
pub struct HostAdapterTypeDescriptor {
    pub name: Name,
    pub implementations: Vec<HostAdapterImplementation>,
}

/// Callback order is determined by the checked declarations, not by native
/// object enumeration. The index selects an exact retained interface type.
#[derive(Clone, Debug)]
pub struct HostAdapterMethod {
    pub implementation_index: u32,
    pub method: Name,
}

/// Immutable registration capability. It owns types and method contracts,
/// never receivers. Clones share these roots without registering another type.
#[derive(Clone, Debug)]
pub struct HostAdapterType {
    class_type: Handle,
    interfaces: Vec<Handle>,
    callbacks: Vec<HostAdapterMethod>,
}

impl HostAdapterType {
    pub fn class_type(&self) -> &Handle {
        &self.class_type
    }

    pub fn callbacks(&self) -> &[HostAdapterMethod] {
        &self.callbacks
    }

    pub fn interfaces(&self) -> &[Handle] {
        &self.interfaces
    }
}

fn mismatch(message: impl Into<String>) -> EngineError {
    EngineError::TypeMismatch {
        message: message.into(),
    }
}

// Handle provenance proves ownership, not that a Class node names a class.
// Check the whole expression before giving its heads to VM substitution.
fn validate_template(
    vm: &bex_vm::BexVm,
    template: &TyTemplate<TypeHead>,
) -> Result<(), EngineError> {
    let mut pending = vec![template.clone()];
    while let Some(ty) = pending.pop() {
        match ty {
            TyTemplate::Class(head, args, _) => {
                let Object::Class(class) = vm.get_object(head.ptr()) else {
                    return Err(mismatch("host adapter Class head is not a class"));
                };
                if args.len() != class.generic_param_count {
                    return Err(mismatch(
                        "host adapter class type has the wrong number of arguments",
                    ));
                }
                pending.extend(args);
            }
            TyTemplate::Interface(head, args, pins, _) => {
                let Object::Interface(interface) = vm.get_object(head.ptr()) else {
                    return Err(mismatch("host adapter Interface head is not an interface"));
                };
                if args.len() != interface.args.len() {
                    return Err(mismatch(
                        "host adapter interface type has the wrong number of arguments",
                    ));
                }
                pending.extend(args);
                pending.extend(pins.into_iter().map(|(_, ty)| ty));
            }
            TyTemplate::Enum(head, _) | TyTemplate::EnumVariant(head, _, _) => {
                if !matches!(vm.get_object(head.ptr()), Object::Enum(_)) {
                    return Err(mismatch("host adapter Enum head is not an enum"));
                }
            }
            TyTemplate::TypeAlias(head, _) => {
                if !matches!(vm.get_object(head.ptr()), Object::TypeAlias(_)) {
                    return Err(mismatch("host adapter alias head is not an alias"));
                }
            }
            TyTemplate::TypeArgRef(index) if index != 0 => {
                return Err(mismatch(
                    "host adapter template only binds Self at slot zero",
                ));
            }
            TyTemplate::List(inner, _) => pending.push(*inner),
            TyTemplate::Map { key, value, .. } => pending.extend([*key, *value]),
            TyTemplate::Union(members, _) => pending.extend(members),
            TyTemplate::Future(value, error, _) => pending.extend([*value, *error]),
            TyTemplate::Function {
                params,
                ret,
                throws,
                ..
            } => {
                pending.extend(params.into_iter().map(|param| param.ty));
                pending.extend([*ret, *throws]);
            }
            TyTemplate::AssociatedTypeProjection {
                base, interface, ..
            } => {
                pending.push(*base);
                pending.push(TyTemplate::interface(
                    interface.name,
                    interface.generics,
                    interface.associated_types,
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

impl BexEngine {
    /// Publish a checked adapter type and root the result before releasing the
    /// heap permit. This operation never acquires or invokes host callbacks.
    pub async fn register_host_adapter(
        self: &Arc<Self>,
        descriptor: HostAdapterTypeDescriptor,
    ) -> Result<HostAdapterType, EngineError> {
        self.register_host_adapter_with_type_arguments(descriptor, Vec::new())
            .await
    }

    /// Frame zero is Self; remaining slots are explicit, fully realized type
    /// evidence shared by every implementation in this registration.
    pub async fn register_host_adapter_with_type_arguments(
        self: &Arc<Self>,
        descriptor: HostAdapterTypeDescriptor,
        type_args: Vec<bex_external_types::TypeArgument>,
    ) -> Result<HostAdapterType, EngineError> {
        let mut thread = self.new_root_thread(CancellationToken::new()).await;
        let mut frame = vec![TyTemplate::TypeArgRef(0)];
        for argument in type_args {
            let (wire, retained) = self.resolve_type_argument(&mut thread, argument)?;
            let ty = match retained {
                Some(value) => value.ty.into(),
                None => crate::conversion::anchor_wire_ty(&thread.vm, &wire)?,
            };
            let realized = RealizedTy::try_from(ty)
                .map_err(|_| mismatch("host adapter type evidence must be fully realized"))?;
            frame.push(realized.into());
        }
        let implementations = descriptor
            .implementations
            .into_iter()
            .map(|implementation| {
                let interface = implementation.interface.try_map_heads(&mut |head| {
                    let ptr = match head {
                        HostDeclaration::Named(name) => {
                            return thread.vm.declaration_head(name).ok_or_else(|| {
                                mismatch(format!("host adapter declaration `{name}` is unavailable"))
                            });
                        }
                        HostDeclaration::Retained(handle) => self
                            .resolve_handle(thread.proof(), handle)
                            .ok_or_else(|| mismatch("host adapter declaration belongs to another runtime or is closed"))?,
                    };
                    let tag = match thread.vm.get_object(ptr) {
                        Object::Class(value) => value.type_tag,
                        Object::Enum(value) => value.type_tag,
                        Object::Interface(value) => value.type_tag,
                        Object::TypeAlias(value) => value.type_tag,
                        _ => return Err(mismatch("host adapter head is not a declaration")),
                    };
                    Ok(TypeHead::new(ptr, tag))
                })?;
                let interface = interface.compose_checked(&frame).map_err(|error| mismatch(error.to_string()))?;
                validate_template(&thread.vm, &interface)?;
                Ok(bex_vm::HostInterfaceImplementation {
                    interface,
                    methods: implementation.methods,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let registration = thread
            .vm
            .register_host_adapter(bex_vm::HostAdapterDescriptor {
                name: descriptor.name,
                implementations,
            })
            .map_err(mismatch)?;
        let class_type = thread.vm.alloc_type(TypeValue::new(registration.class));
        let class_type = self.heap.create_handle(class_type);
        let interfaces = registration
            .interfaces
            .into_iter()
            .map(|interface| {
                let ptr = thread.vm.alloc_type(TypeValue::new(interface));
                self.heap.create_handle(ptr)
            })
            .collect();
        let callbacks = registration
            .callbacks
            .into_iter()
            .map(|slot| HostAdapterMethod {
                implementation_index: slot.implementation_index,
                method: slot.method,
            })
            .collect();
        Ok(HostAdapterType {
            class_type,
            interfaces,
            callbacks,
        })
    }

    /// Create an independently owned instance of a registered adapter. The
    /// original receiver is retained even when there are zero supplied methods.
    /// Rejection drops the incoming references without invoking a host body.
    pub async fn create_host_adapter_instance(
        self: &Arc<Self>,
        registration: &HostAdapterType,
        receiver: Arc<HostValueArc>,
        callbacks: Vec<Arc<HostValueArc>>,
    ) -> Result<Handle, EngineError> {
        let mut thread = self.new_root_thread(CancellationToken::new()).await;
        let ptr = self
            .resolve_handle(thread.proof(), &registration.class_type)
            .ok_or_else(|| mismatch("host adapter type belongs to another runtime or is closed"))?;
        let Object::Type(value) = thread.vm.get_object(ptr) else {
            return Err(mismatch("host adapter registration does not retain a type"));
        };
        let class: RealizedTy = value.ty.clone();
        let value = thread
            .vm
            .create_host_adapter_instance(&class, receiver, callbacks)
            .map_err(mismatch)?;
        let ValueKind::Object(ptr) = value.kind() else {
            return Err(mismatch("host adapter instance is not a heap object"));
        };
        Ok(self.heap.create_handle(ptr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_heap::CollectionLevel;
    use bex_resource_types::HostValueKind;
    use sys_native::SysOpsExt;

    fn engine() -> Arc<BexEngine> {
        Arc::new(
            BexEngine::new(
                baml_db::testing::compile_source(
                    "interface Marker {}\ninterface Echo { type Item function echo(self, value: Self.Item) -> Self.Item throws never }",
                ),
                Arc::new(sys_native::SysOps::native()),
                Vec::new(),
            )
            .unwrap(),
        )
    }

    fn marker() -> HostAdapterTypeDescriptor {
        HostAdapterTypeDescriptor {
            name: Name::new("Adapter"),
            implementations: vec![HostAdapterImplementation {
                interface: TyTemplate::interface(
                    HostDeclaration::Named(TypeName::from_dotted_path("user.Marker")),
                    vec![],
                    vec![],
                ),
                methods: vec![],
            }],
        }
    }

    #[tokio::test]
    async fn owned_host_type_survives_gc_without_retaining_dead_instances() {
        let engine = engine();
        let original = engine.register_host_adapter(marker()).await.unwrap();
        let registration = original.clone();
        drop(original);
        engine.collect_garbage(CollectionLevel::Major).await;
        assert!(registration.callbacks().is_empty());
        for key in [10, 11] {
            let receiver = HostValueArc::new(key, HostValueKind::Opaque);
            let weak = Arc::downgrade(&receiver);
            let instance = engine
                .create_host_adapter_instance(&registration, receiver, vec![])
                .await
                .unwrap();
            engine.collect_garbage(CollectionLevel::Major).await;
            assert!(weak.upgrade().is_some());
            drop(instance);
            engine.collect_garbage(CollectionLevel::Major).await;
            assert!(
                weak.upgrade().is_none(),
                "live type must not root instances"
            );
        }
    }

    #[tokio::test]
    async fn host_registration_rejects_foreign_roots_and_invalid_receivers() {
        let first = engine();
        let second = engine();
        let registration = first.register_host_adapter(marker()).await.unwrap();
        let receiver = HostValueArc::new(12, HostValueKind::Opaque);
        let weak = Arc::downgrade(&receiver);
        let error = second
            .create_host_adapter_instance(&registration, receiver, vec![])
            .await
            .unwrap_err();
        assert!(error.to_string().contains("another runtime"));
        assert!(weak.upgrade().is_none());

        let receiver = HostValueArc::new(13, HostValueKind::Callable);
        let weak = Arc::downgrade(&receiver);
        let error = first
            .create_host_adapter_instance(&registration, receiver, vec![])
            .await
            .unwrap_err();
        assert!(error.to_string().contains("opaque host registration"));
        assert!(weak.upgrade().is_none());

        let mut descriptor = marker();
        // A Type wrapper is not a declaration; accepting it as a head would
        // reinterpret a heap object. The same handle is also foreign to second.
        descriptor.implementations[0].interface = TyTemplate::interface(
            HostDeclaration::Retained(registration.class_type().clone()),
            vec![],
            vec![],
        );
        let error = first
            .register_host_adapter(descriptor.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not a declaration"));
        let error = second.register_host_adapter(descriptor).await.unwrap_err();
        assert!(error.to_string().contains("another runtime"));
    }

    #[tokio::test]
    async fn host_callback_contract_retains_dynamic_associated_identity() {
        let engine = engine();
        let item = engine.register_host_adapter(marker()).await.unwrap();
        let (declaration, tag) = {
            let thread = engine.new_root_thread(CancellationToken::new()).await;
            let ptr = engine
                .resolve_handle(thread.proof(), item.class_type())
                .unwrap();
            let Object::Type(value) = thread.vm.get_object(ptr) else {
                panic!()
            };
            let RealizedTy::Class(head, _, _) = &value.ty else {
                panic!()
            };
            (engine.heap.create_handle(head.ptr()), head.tag())
        };
        let adapter = engine
            .register_host_adapter(HostAdapterTypeDescriptor {
                name: Name::new("EchoAdapter"),
                implementations: vec![HostAdapterImplementation {
                    interface: TyTemplate::interface(
                        HostDeclaration::Named(TypeName::from_dotted_path("user.Echo")),
                        vec![],
                        vec![(
                            Name::new("Item"),
                            TyTemplate::class(HostDeclaration::Retained(declaration), vec![]),
                        )],
                    ),
                    methods: vec![Name::new("echo")],
                }],
            })
            .await
            .unwrap();
        drop(item);
        engine.collect_garbage(CollectionLevel::Major).await;
        let instance = engine
            .create_host_adapter_instance(
                &adapter,
                HostValueArc::new(20, HostValueKind::Opaque),
                vec![HostValueArc::new(21, HostValueKind::Callable)],
            )
            .await
            .unwrap();
        let view = engine
            .project_interface_with_type_argument(
                crate::BexExternalValue::Handle(instance),
                bex_external_types::TypeArgument::Reference(adapter.interfaces()[0].clone()),
            )
            .await
            .unwrap();
        let baml_type::RealizedTy::Interface(_, _, pins, _) = &view.interface else {
            panic!()
        };
        let baml_type::RealizedTy::Class(head, _, _) = &pins[0].1 else {
            panic!()
        };
        assert_eq!(head.tag(), tag);
        let thread = engine.new_root_thread(CancellationToken::new()).await;
        let ptr = engine
            .resolve_handle(thread.proof(), &adapter.interfaces()[0])
            .unwrap();
        let Object::Type(value) = thread.vm.get_object(ptr) else {
            panic!()
        };
        let RealizedTy::Interface(_, _, pins, _) = &value.ty else {
            panic!()
        };
        let RealizedTy::Class(head, _, _) = &pins[0].1 else {
            panic!()
        };
        assert_eq!(
            head.tag(),
            tag,
            "retained identity must not be reconstructed by name"
        );
        assert!(matches!(thread.vm.get_object(head.ptr()), Object::Class(_)));
    }

    #[tokio::test]
    async fn host_template_preserves_self_and_rejects_malformed_nested_heads() {
        let engine = engine();
        let descriptor = |item| HostAdapterTypeDescriptor {
            name: Name::new("SelfEcho"),
            implementations: vec![HostAdapterImplementation {
                interface: TyTemplate::interface(
                    HostDeclaration::Named(TypeName::from_dotted_path("user.Echo")),
                    vec![],
                    vec![(Name::new("Item"), item)],
                ),
                methods: vec![Name::new("echo")],
            }],
        };
        let malformed = TyTemplate::list(TyTemplate::class(
            HostDeclaration::Named(TypeName::from_dotted_path("user.Marker")),
            vec![],
        ));
        let error = engine
            .register_host_adapter(descriptor(malformed))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("Class head is not a class"));
        let error = engine
            .register_host_adapter(descriptor(TyTemplate::TypeArgRef(1)))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("frame"));
        let registration = engine
            .register_host_adapter(descriptor(TyTemplate::TypeArgRef(0)))
            .await
            .unwrap();
        engine.collect_garbage(CollectionLevel::Major).await;
        let thread = engine.new_root_thread(CancellationToken::new()).await;
        let class = engine
            .resolve_handle(thread.proof(), registration.class_type())
            .unwrap();
        let interface = engine
            .resolve_handle(thread.proof(), &registration.interfaces()[0])
            .unwrap();
        let Object::Type(class) = thread.vm.get_object(class) else {
            panic!()
        };
        let Object::Type(interface) = thread.vm.get_object(interface) else {
            panic!()
        };
        let RealizedTy::Interface(_, _, pins, _) = &interface.ty else {
            panic!()
        };
        assert_eq!(pins[0].1, class.ty);
    }
}
