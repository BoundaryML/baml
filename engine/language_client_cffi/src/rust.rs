use baml_types::BamlMedia;

use crate::{baml::cffi::BamlObjectHandle, ctypes::Encode, raw_ptr_wrapper::RawPtrType};

/// Safe Rust-facing handle for any raw pointer object managed by the CFFI layer.
#[derive(Clone, Debug)]
pub struct RawObjectHandle {
    raw: RawPtrType,
}

impl RawObjectHandle {
    pub(crate) fn new(raw: RawPtrType) -> Self {
        Self { raw }
    }

    /// Clone the internal raw pointer representation.
    pub fn raw(&self) -> RawPtrType {
        self.raw.clone()
    }

    /// Convert the handle into the protobuf representation used by FFI calls.
    pub fn to_cffi(&self) -> BamlObjectHandle {
        self.raw.clone().encode()
    }
}

/// Handle for collector objects.
#[derive(Clone, Debug)]
pub struct CollectorHandle {
    handle: RawObjectHandle,
}

impl CollectorHandle {
    pub fn new(name: Option<&str>) -> Result<Self, String> {
        let raw = RawPtrType::create_collector(name)?;
        Ok(Self {
            handle: RawObjectHandle::new(raw),
        })
    }

    pub fn to_cffi(&self) -> BamlObjectHandle {
        self.handle.to_cffi()
    }

    pub fn raw(&self) -> RawPtrType {
        self.handle.raw()
    }
}

/// Handle for type builder objects.
#[derive(Clone, Debug)]
pub struct TypeBuilderHandle {
    handle: RawObjectHandle,
}

impl TypeBuilderHandle {
    pub fn new() -> Result<Self, String> {
        let raw = RawPtrType::create_type_builder()?;
        Ok(Self {
            handle: RawObjectHandle::new(raw),
        })
    }

    pub fn to_cffi(&self) -> BamlObjectHandle {
        self.handle.to_cffi()
    }

    pub fn raw(&self) -> RawPtrType {
        self.handle.raw()
    }
}

/// Helper to create media raw objects from `BamlMedia` instances.
pub fn media_to_raw(media: &BamlMedia) -> BamlObjectHandle {
    let raw: RawPtrType = RawPtrType::from(media.clone());
    raw.encode()
}
