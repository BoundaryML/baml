use std::collections::HashMap;

use baml_runtime::client_registry::ClientRegistry;
use baml_types::BamlValue;

use crate::{
    ctypes::cffi_generated::cffi::{CFFICollector, CFFIFunctionArguments},
    raw_ptr_wrapper::CollectorWrapper,
};

pub struct BamlFunctionArguments {
    pub kwargs: baml_types::BamlMap<String, BamlValue>,
    pub client_registry: Option<ClientRegistry>,
    pub env_vars: HashMap<String, String>,
    pub collectors: Option<Vec<CollectorWrapper>>,
}

use super::traits::Decode;

impl Decode for BamlFunctionArguments {
    type From<'a> = CFFIFunctionArguments<'a>;

    fn decode(from: Self::From<'_>) -> Result<Self, anyhow::Error> {
        let kwargs = from
            .kwargs()
            .map(|v| v.iter().map(|v| v.into()).collect())
            .unwrap_or_default();
        let client_registry = from.client_registry().map(|r| r.into());
        let env_vars = from
            .env()
            .map(|e| e.iter().map(|v| v.into()).collect())
            .unwrap_or_default();
        let collectors = from
            .collectors()
            .map(|c| {
                c.iter()
                    .map(|c| CollectorWrapper::decode(c))
                    .collect::<Result<_, _>>()
            })
            .transpose()?;

        println!("collectors: {:?}", collectors);
        Ok(BamlFunctionArguments {
            kwargs,
            client_registry,
            env_vars,
            collectors,
        })
    }
}

impl Decode for CollectorWrapper {
    type From<'a> = CFFICollector<'a>;

    fn decode(from: Self::From<'_>) -> Result<Self, anyhow::Error> {
        match from.pointer() {
            0 => Err(anyhow::anyhow!("Collector pointer is 0")),
            ptr => Ok(CollectorWrapper::from_raw(ptr as *const libc::c_void, true)),
        }
    }
}
