//! Owning projection of an opaque generated `$rust_type` field.
use std::sync::Arc;

use crate::{
    DecodeError,
    baml_value::internal::{__BamlValuePrivate, EncodeError, Encoder},
    wire,
};

#[derive(Clone)]
pub struct RustType(Arc<OwnedHandle>);
pub(crate) struct OwnedHandle(pub(crate) u64);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // This object owns exactly one reference.
        crate::handle::release_engine_key(self.0);
    }
}
impl std::fmt::Debug for RustType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RustType { .. }")
    }
}
impl RustType {
    #[doc(hidden)]
    pub(crate) fn invocation_key(&self) -> u64 {
        self.0.0
    }
}
impl __BamlValuePrivate for RustType {
    fn to_baml(&self, encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        Ok(wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::Handle(wire::BamlHandle {
                key: encoder.clone_handle(self.0.0)?,
                handle_type: wire::BamlHandleType::UntaggedRustData as i32,
            })),
        })
    }
    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let value = crate::decode::unwrap(value);
        let Some(wire::baml_outbound_value::Value::HandleValue(handle)) = value.value else {
            return Err(DecodeError::WrongType {
                expected: "RustType",
                got: "value",
            });
        };
        if handle.handle_type != wire::BamlHandleType::UntaggedRustData as i32 || handle.key == 0 {
            return Err(DecodeError::WrongType {
                expected: "RustType",
                got: "handle",
            });
        }
        Ok(Self(Arc::new(OwnedHandle(handle.key))))
    }
    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::RustType(wire::BamlTyRustType {})),
        }
    }
}

impl PartialEq for RustType {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.0 == other.0.0
    }
}
