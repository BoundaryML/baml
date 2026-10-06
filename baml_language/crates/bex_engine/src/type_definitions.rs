//! The engine's telemetry definition resolver. The recorder polls it on the
//! processor thread, which must keep draining transport no matter what the
//! heap is doing: a VM holding a permit can be waiting for transport space
//! while a collection waits for that VM. So the resolver never waits for heap
//! access. It takes a permit only when one is free right now, copies a
//! bounded batch of definitions into owned values, and releases the permit
//! before handing anything over; otherwise it answers `Busy` and keeps its
//! work. Definitions of declarations collected meanwhile were extracted by
//! the collector and need no permit at all.

use std::sync::{Arc, Mutex};

use bex_heap::{BexHeap, HeapPermit, HeapPermitManager, InactiveHeapPermit};
use btel_types::{TypeDefinitionSource, TypeResolution};

pub(crate) struct HeapTypeDefinitions {
    heap: Arc<BexHeap>,
    permits: Arc<HeapPermitManager>,
    /// Registered once, kept inactive between polls. It holds no roots.
    permit: Mutex<Option<InactiveHeapPermit<()>>>,
}

impl HeapTypeDefinitions {
    pub(crate) fn new(heap: Arc<BexHeap>, permits: Arc<HeapPermitManager>) -> Self {
        Self {
            heap,
            permits,
            permit: Mutex::new(None),
        }
    }
}

impl TypeDefinitionSource for HeapTypeDefinitions {
    fn try_take(&self, max: usize) -> TypeResolution {
        let mut definitions = self.heap.take_settled_type_definitions();
        let ready = |definitions, more| TypeResolution::Ready { definitions, more };
        if !self.heap.has_type_definition_work() {
            return ready(definitions, false);
        }
        let busy = |definitions: Vec<_>| {
            if definitions.is_empty() {
                TypeResolution::Busy
            } else {
                ready(definitions, true)
            }
        };
        let Ok(mut slot) = self.permit.try_lock() else {
            return busy(definitions);
        };
        let inactive = match slot.take() {
            Some(permit) => permit,
            None => match self.permits.try_new_permit(()) {
                Some(permit) => permit,
                None => return busy(definitions),
            },
        };
        match inactive.try_acquire() {
            Ok(active) => {
                definitions.extend(self.heap.resolve_type_definitions(active.proof(), max));
                // Release before anything leaves: the copies are owned.
                *slot = Some(active.release());
                let more = self.heap.has_type_definition_work();
                ready(definitions, more)
            }
            Err(inactive) => {
                *slot = Some(inactive);
                busy(definitions)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    use baml_test_support::compile_source;
    use bex_vm_types::Object;
    use btel_types::{TypeDefinitionSource, TypeResolution};
    use sys_native::SysOpsExt;

    use super::HeapTypeDefinitions;
    use crate::BexEngine;

    fn tag_of(ptr: bex_vm_types::HeapPtr) -> baml_type::typetag::TypeTag {
        // SAFETY: a compile-time declaration, never moved or collected.
        match unsafe { ptr.get() } {
            Object::Class(class) => class.type_tag,
            Object::Enum(enm) => enm.type_tag,
            _ => unreachable!("a declaration"),
        }
    }

    /// The resolver never waits for the heap: while a collection holds it
    /// exclusively, both before and after the resolver registered its own
    /// permit, it answers Busy at once and keeps the work, which resolves
    /// once the heap is free.
    #[tokio::test]
    async fn resolution_is_busy_while_a_collection_holds_the_heap() {
        let program = compile_source(
            "class Foo { value int @description(\"v\") }\nenum Bar { A B }\nfunction main() -> null { null }",
        );
        let engine = BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine");
        let foo = *engine.resolved_class_names.get("user.Foo").expect("Foo");
        let bar = *engine.resolved_enum_names.get("user.Bar").expect("Bar");
        let source = HeapTypeDefinitions::new(
            Arc::clone(&engine.heap),
            Arc::clone(&engine.heap_permit_manager),
        );
        let register = |ptr| {
            // SAFETY: compile-time declarations; no collection runs here.
            unsafe { engine.heap.register_telemetry_declaration(tag_of(ptr), ptr) };
        };

        register(foo);
        {
            // Exclusive access before the resolver has a permit: the
            // permit registry is held, so registration does not wait.
            let guard = engine.heap_permit_manager.request_park().await;
            let started = Instant::now();
            assert!(matches!(source.try_take(16), TypeResolution::Busy));
            assert!(started.elapsed() < Duration::from_millis(200));
            drop(guard);
        }
        let TypeResolution::Ready { definitions, more } = source.try_take(16) else {
            panic!("free heap resolves");
        };
        assert!(!more);
        let [definition] = definitions.as_slice() else {
            panic!("{definitions:?}");
        };
        let declaration = definition.declaration.as_ref().expect("Foo's declaration");
        assert_eq!(declaration.fields[0].name, "value");
        assert_eq!(declaration.fields[0].description.as_deref(), Some("v"));

        register(bar);
        {
            // Exclusive access after the resolver registered its permit:
            // activating it does not wait either.
            let guard = engine.heap_permit_manager.request_park().await;
            let started = Instant::now();
            assert!(matches!(source.try_take(16), TypeResolution::Busy));
            assert!(started.elapsed() < Duration::from_millis(200));
            drop(guard);
        }
        let TypeResolution::Ready { definitions, .. } = source.try_take(16) else {
            panic!("free heap resolves");
        };
        let variants: Vec<_> = definitions[0]
            .declaration
            .as_ref()
            .unwrap()
            .variants
            .iter()
            .map(|v| v.name.clone())
            .collect();
        assert_eq!(variants, ["A", "B"]);
    }
}
