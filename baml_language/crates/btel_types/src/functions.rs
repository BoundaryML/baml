//! Owned, engine-scoped function identity and metadata. No VM heap references.

use std::{
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

/// Eight-byte runtime function identity, scoped by its engine. Never reused.
/// `Option<FunctionId>` is also eight bytes; `None` means telemetry unsupported.
/// Compiler templates and deserialized functions receive an ID when loaded as
/// executable definitions; telemetry lookup registration can happen later.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(NonZeroU64);

impl FunctionId {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

const _: () = assert!(std::mem::size_of::<FunctionId>() == 8);
const _: () = assert!(std::mem::size_of::<Option<FunctionId>>() == 8);

/// The engine cannot allocate another function ID without reusing an identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionIdExhausted;

impl std::fmt::Display for FunctionIdExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("runtime function identity space exhausted")
    }
}
impl std::error::Error for FunctionIdExhausted {}

/// Stable key naming a definition site (`function:pkg.f`, `class:pkg.C`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionKey(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFunctionKind {
    Bytecode,
    SysOp(String),
    Native,
    NativeUnresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    Builtin,
    AutoDerive,
}

/// Metadata for one compiled function, identified within this program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionMetadata {
    pub function_id: FunctionId,
    pub fqn: String,
    pub display_name: String,
    pub source_file: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub kind: RuntimeFunctionKind,
    pub origin: RuntimeFunctionOrigin,
    pub owner_type: Option<DefinitionKey>,
    pub parent_function: Option<DefinitionKey>,
    pub lambda_path: Option<String>,
    pub definition_key: Option<DefinitionKey>,
    pub package_name: Option<String>,
    pub namespace: Vec<String>,
}

/// Owned snapshot of available functions, sorted by ID. Collected IDs may be absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionMetadataTable {
    pub functions: Vec<FunctionMetadata>,
}

impl FunctionMetadataTable {
    #[must_use]
    pub fn get(&self, function_id: FunctionId) -> Option<&FunctionMetadata> {
        self.functions
            .binary_search_by_key(&function_id, |entry| entry.function_id)
            .ok()
            .map(|index| &self.functions[index])
    }
}

/// Allocates engine-scoped function identities without retaining definitions.
/// Exhaustion is permanent: collection never makes an ID available for reuse.
#[derive(Debug, Default)]
pub struct FunctionIdAllocator {
    last: AtomicU64,
}

impl FunctionIdAllocator {
    pub fn allocate(&self) -> Result<FunctionId, FunctionIdExhausted> {
        let previous = self
            .last
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| {
                last.checked_add(1)
            })
            .map_err(|_| FunctionIdExhausted)?;
        Ok(FunctionId(
            NonZeroU64::new(previous + 1).expect("increment is nonzero"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_ids_do_not_alias_static_functions() {
        let mut lookup = crate::FunctionLookup::default();
        let static_id = lookup
            .register_static(10, &crate::FunctionRegistration::default())
            .unwrap();
        // Low 32 bits equal the static ID: truncation on wasm32 would alias it.
        let high_id = FunctionId(NonZeroU64::new((1u64 << 32) | static_id.get()).unwrap());
        assert_eq!(lookup.get(high_id), None);
        lookup.register(high_id, 20, &crate::FunctionRegistration::default());
        assert_eq!(lookup.get(high_id), Some(20));
        assert_eq!(lookup.get(static_id), Some(10));
    }

    #[test]
    fn exhaustion_never_wraps_or_reuses_an_id() {
        assert_eq!(FunctionIdAllocator::default().allocate().unwrap().get(), 1);
        let allocator = FunctionIdAllocator {
            last: AtomicU64::new(u64::MAX - 1),
        };
        assert_eq!(allocator.allocate().unwrap().get(), u64::MAX);
        for _ in 0..2 {
            assert_eq!(allocator.allocate(), Err(FunctionIdExhausted));
        }
    }
}
