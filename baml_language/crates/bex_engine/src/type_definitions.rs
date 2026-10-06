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

/// The handoff's deadlock, end to end: a VM holds a heap permit while it
/// writes into a full transport, a collection queues behind that permit,
/// and definitions are pending. The processor must keep draining (the
/// resolver never waits for the heap), so the VM's writes complete, then the
/// collection runs, and the pending definition is recorded at the end.
#[cfg(test)]
mod deadlock_tests {
    use std::{
        num::NonZeroUsize,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use baml_test_support::compile_source;
    use bex_vm_types::Object;
    use btel_records::SpanRecord;
    use sys_native::SysOpsExt;

    use super::HeapTypeDefinitions;
    use crate::BexEngine;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn draining_never_waits_for_a_collection_queued_behind_a_writer() {
        let program = compile_source(
            "class Foo { value int @description(\"v\") }\nfunction main() -> null { null }",
        );
        let engine = Arc::new(
            BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
                .expect("engine"),
        );
        let foo = *engine.resolved_class_names.get("user.Foo").expect("Foo");
        // SAFETY: a compile-time declaration, never moved or collected.
        let Object::Class(class) = (unsafe { foo.get() }) else {
            unreachable!("a class")
        };
        // SAFETY: a compile-time declaration; no collection runs yet.
        unsafe {
            engine
                .heap
                .register_telemetry_declaration(class.type_tag, foo);
        }

        // A transport of four chunks of eight records per lane.
        let files: Arc<Mutex<Vec<btel_recorder::SealedFile>>> = Arc::default();
        let sink = Arc::clone(&files);
        let publisher = btel_recorder::RecordingPublisher::new(
            btel_recorder::RecordingId::generate(),
            btel_recorder::RecordingConfig::default(),
            move |file| sink.lock().unwrap().push(file),
        )
        .unwrap()
        .with_type_definitions(Arc::new(HeapTypeDefinitions::new(
            Arc::clone(&engine.heap),
            Arc::clone(&engine.heap_permit_manager),
        )));
        let runtime = btel_processor::TelemetryRuntime::with_config_and_publisher(
            bex_chunkedringbuffer::Config {
                chunk_capacity: NonZeroUsize::new(8).unwrap(),
                timing_chunks: NonZeroUsize::new(4).unwrap(),
                span_chunks: NonZeroUsize::new(4).unwrap(),
                max_producers: NonZeroUsize::new(2).unwrap(),
                preallocate: true,
            },
            publisher,
        )
        .unwrap();

        // The VM: holds an active permit and writes far more than the
        // transport holds, so it waits on the processor to drain.
        let vm_permit = engine
            .heap_permit_manager
            .new_permit(())
            .await
            .acquire()
            .await;
        let writer = {
            let runtime = Arc::clone(&runtime);
            std::thread::spawn(move || {
                let scope = runtime.enter();
                let thread = btel_types::allocate_telemetry_id();
                for _ in 0..2_000 {
                    runtime.write_span(
                        thread,
                        SpanRecord::ThreadSpanRunning {
                            at: btel_types::ClockInstant::from_ticks(1),
                        },
                    );
                }
                drop(scope);
            })
        };
        // The collection: queued behind the VM's permit.
        let permits = Arc::clone(&engine.heap_permit_manager);
        let collection = tokio::spawn(async move {
            let guard = permits.request_park().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            drop(guard);
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!collection.is_finished(), "the collection waits for the VM");

        // The writes complete while the collection is still queued: the
        // processor drained without waiting on the heap.
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !writer.is_finished() {
            assert!(std::time::Instant::now() < deadline, "processor stalled");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        writer.join().unwrap();
        assert!(!collection.is_finished(), "still queued behind the VM");

        drop(vm_permit);
        tokio::time::timeout(Duration::from_secs(30), collection)
            .await
            .expect("the collection runs once the VM releases its permit")
            .unwrap();

        let finish = Arc::clone(&runtime);
        tokio::task::spawn_blocking(move || finish.finish())
            .await
            .unwrap()
            .unwrap();
        let types: Vec<_> = files
            .lock()
            .unwrap()
            .iter()
            .filter_map(|file| {
                <btel_recorder::proto::RecordingFile as prost::Message>::decode(file.bytes()).ok()
            })
            .flat_map(|file| file.definitions.map(|d| d.types).unwrap_or_default())
            .collect();
        let declared = types.iter().any(|definition| {
            definition.type_tag == class.type_tag.as_i64()
                && matches!(
                    definition.resolution,
                    Some(btel_recorder::proto::type_definition::Resolution::Declaration(_))
                )
        });
        assert!(declared, "the pending definition is recorded: {types:?}");
    }
}
