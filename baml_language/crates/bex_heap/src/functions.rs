//! Function identity lookup owned by the engine's heap. Dynamic entries are
//! weak: they never participate in tracing and are repaired/pruned by GC.

use std::collections::HashMap;

use bex_vm_types::{HeapPtr, Object, PermitProof};
use btel_types::{FunctionId, FunctionMetadata, FunctionMetadataTable};

use crate::{BexHeap, CollectionLevel, Generation};

impl BexHeap {
    /// Register a function before publishing its first telemetry reference.
    /// Return its stable ID after registration; unsupported functions return None.
    ///
    /// # Safety
    /// The caller must exclude GC, and the pointer must belong to this heap
    /// and refer to a fully initialized function. Linking must have finished.
    #[inline]
    pub unsafe fn register_telemetry_function(&self, ptr: HeapPtr) -> Option<FunctionId> {
        // SAFETY: caller guarantees the live, completed object and excludes GC.
        let Object::Function(function) = (unsafe { ptr.get() }) else {
            return None;
        };
        let id = function.telemetry_function_id?;
        self.functions
            .register(id, ptr, &function.telemetry_registration);
        Some(id)
    }

    /// Return an owned copy; never expose a weak pointer beyond the permit.
    pub fn function_metadata(
        &self,
        _proof: PermitProof<'_>,
        id: FunctionId,
    ) -> Option<FunctionMetadata> {
        let ptr = self.functions.get(id)?;
        // SAFETY: the active permit excludes GC; registration publishes only
        // initialized functions, and collection updates/removes weak entries.
        let Object::Function(function) = (unsafe { ptr.get() }) else {
            unreachable!("function lookup contains a non-function")
        };
        debug_assert_eq!(function.telemetry_function_id, Some(id));
        function.runtime_metadata()
    }

    /// Copy available metadata on demand. Collected IDs leave gaps, so the
    /// snapshot is sorted by ID rather than treating its offsets as IDs.
    pub fn function_metadata_snapshot(&self, _proof: PermitProof<'_>) -> FunctionMetadataTable {
        // Release the lookup locks before copying metadata. The permit keeps
        // all references valid until their owned metadata has been constructed.
        let mut functions: Vec<_> = self
            .functions
            .references()
            .into_iter()
            .map(|ptr| {
                // SAFETY: same permit/registration/collection invariant as lookup.
                let Object::Function(function) = (unsafe { ptr.get() }) else {
                    unreachable!("function lookup contains a non-function")
                };
                function
                    .runtime_metadata()
                    .expect("registered function has an ID")
            })
            .collect();
        functions.sort_unstable_by_key(|metadata| metadata.function_id);
        FunctionMetadataTable { functions }
    }

    /// Metadata for compile-time functions only. They are never moved or
    /// collected after sealing, so no permit is needed. Sorted by ID; dynamic
    /// functions (runtime-compiled or grafted) are absent.
    pub fn static_function_metadata(&self) -> FunctionMetadataTable {
        let functions = (0..self.compile_time_len())
            .filter_map(|index| match self.compile_time_object(index) {
                Object::Function(function) => function.runtime_metadata(),
                _ => None,
            })
            .collect::<Vec<_>>();
        debug_assert!(functions.is_sorted_by_key(|f| f.function_id));
        FunctionMetadataTable { functions }
    }

    /// Only called at the GC safepoint, after tracing/finalizer preservation
    /// and before swapping or destroying from-space. Never marks anything live.
    pub(crate) fn update_function_lookup(
        &self,
        forwarding: &HashMap<HeapPtr, HeapPtr>,
        level: CollectionLevel,
    ) {
        self.functions.retain_dynamic(|ptr| {
            if let Some(&new_ptr) = forwarding.get(ptr) {
                *ptr = new_ptr;
                true
            } else {
                level == CollectionLevel::Minor && self.generation_of(*ptr) == Generation::Gen2
            }
        });
    }
}
