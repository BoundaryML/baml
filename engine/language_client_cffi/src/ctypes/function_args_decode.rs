use std::collections::HashMap;

use baml_runtime::client_registry::{ClientProperty, ClientProvider, ClientRegistry};
use baml_types::BamlValue;

use super::utils::Decode;
use crate::raw_ptr_wrapper::{CollectorWrapper, RawPtrType, TypeBuilderWrapper};

pub struct BamlFunctionArguments {
    pub kwargs: baml_types::BamlMap<String, BamlValue>,
    pub client_registry: Option<ClientRegistry>,
    pub env_vars: HashMap<String, String>,
    pub collectors: Option<Vec<CollectorWrapper>>,
    pub type_builder: Option<TypeBuilderWrapper>,
}

impl Decode for BamlFunctionArguments {
    type From = crate::baml::cffi::CffiFunctionArguments;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        let kwargs = from
            .kwargs
            .into_iter()
            .map(|v| match v.value {
                Some(value) => Ok((v.key, BamlValue::decode(value)?)),
                None => Err(anyhow::anyhow!("Failed to decode BamlValue")),
            })
            .collect::<Result<_, _>>()?;
        let client_registry = from
            .client_registry
            .map(ClientRegistry::decode)
            .transpose()?
            .filter(|r| !r.is_empty());
        let env_vars = from.env.into_iter().map(|e| (e.key, e.value)).collect();
        let collectors = {
            let collectors = from
                .collectors
                .into_iter()
                .map(RawPtrType::decode)
                .map(|r| match r {
                    Ok(RawPtrType::Collector(c)) => Ok(c),
                    Err(e) => Err(e),
                    Ok(other) => Err(anyhow::anyhow!("Expected Collector, got {}", other.name())),
                })
                .collect::<Result<Vec<_>, _>>()?;
            if collectors.is_empty() {
                None
            } else {
                Some(collectors)
            }
        };
        let type_builder = from
            .type_builder
            .map(RawPtrType::decode)
            .transpose()?
            .map(|r| match r {
                RawPtrType::TypeBuilder(t) => Ok(t),
                other => Err(anyhow::anyhow!(
                    "Expected TypeBuilder, got {}",
                    other.name()
                )),
            })
            .transpose()?;

        Ok(BamlFunctionArguments {
            kwargs,
            client_registry,
            env_vars,
            collectors,
            type_builder,
        })
    }
}

impl Decode for ClientRegistry {
    type From = crate::baml::cffi::CffiClientRegistry;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        let mut client_registry = ClientRegistry::new();
        if let Some(p) = from.primary {
            client_registry.set_primary(p)
        }
        from.clients
            .into_iter()
            .map(ClientProperty::decode)
            .try_for_each(|f| f.map(|f| client_registry.add_client(f)))?;

        Ok(client_registry)
    }
}

impl Decode for ClientProperty {
    type From = crate::baml::cffi::CffiClientProperty;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        let options = from
            .options
            .into_iter()
            .map(super::baml_value_decode::from_cffi_map_entry)
            .collect::<Result<_, _>>()?;
        let provider = from.provider.parse::<ClientProvider>()?;
        Ok(ClientProperty::new(
            from.name,
            provider,
            from.retry_policy,
            options,
        ))
    }
}
