use ::bex_sap::sap_model;
use ::sys_types::DefKey;

/// Cached schema information for incremental or one-shot SAP parsing.
pub struct SapParseCache {
    types: bex_sap::CompiledSapModel,
}

impl SapParseCache {
    pub fn new(types: bex_sap::CompiledSapModel) -> Self {
        Self { types }
    }

    pub fn db(&self) -> &sap_model::TypeRefDb<'_, DefKey> {
        self.types.db()
    }

    pub fn ty(&self) -> &sap_model::AnnotatedTy<'_, DefKey> {
        self.types.ty()
    }

    pub fn ty_resolved(
        &self,
    ) -> Result<
        sap_model::TyWithMeta<
            sap_model::TyResolvedRef<'_, DefKey>,
            &sap_model::TypeAnnotations<'_, DefKey>,
        >,
        &DefKey,
    > {
        self.db().resolve_with_meta(self.ty().as_ref())
    }

    pub fn stream_ty_resolved(
        &self,
    ) -> Result<
        sap_model::TyWithMeta<
            sap_model::TyResolvedRef<'_, DefKey>,
            &sap_model::TypeAnnotations<'_, DefKey>,
        >,
        &DefKey,
    > {
        self.db().resolve_with_meta(self.types.stream_ty().as_ref())
    }
}

/// Errors that can occur during LLM operations. Relocated verbatim from
/// `sys_llm`; only `ParseResponseError`, `JsonishError` and `SapError` are
/// still constructible now that the SAP parse entry points are the sole users.
#[derive(Debug, thiserror::Error)]
pub enum LlmOpError {
    #[error("Expected {expected}, got {actual}")]
    TypeError {
        expected: &'static str,
        actual: String,
    },

    #[error("Parse response error: {0}")]
    ParseResponseError(String),

    #[error("Jsonish error: {0}")]
    JsonishError(::bex_sap::jsonish::JsonishError),

    #[error("SAP error: {0}")]
    SapError(::bex_sap::deserializer::coercer::ParsingError),
}

impl From<LlmOpError> for ::sys_types::VmRustFnError {
    fn from(e: LlmOpError) -> Self {
        let baml: ::sys_types::VmBamlError = match e {
            LlmOpError::TypeError { expected, actual } => {
                ::sys_types::VmBamlError::InvalidArgument {
                    message: format!("expected {expected}, got {actual}"),
                }
            }
            LlmOpError::ParseResponseError(e) => ::sys_types::VmBamlError::LlmClient { message: e },
            LlmOpError::JsonishError(e) => ::sys_types::VmBamlError::LlmClient {
                message: e.to_string(),
            },
            LlmOpError::SapError(e) => ::sys_types::VmBamlError::LlmClient {
                message: e.to_string(),
            },
        };
        baml.into()
    }
}

pub fn execute_sap_parse_final(
    json: &str,
    sap: &SapParseCache,
    _ctx: &::sys_types::SysOpContext,
) -> Result<bex_external_types::BexExternalValue, LlmOpError> {
    // === Jsonish ===
    let jsonish_options = ::bex_sap::jsonish::ParseOptions::default();
    let jsonish =
        ::bex_sap::jsonish::parse(json, jsonish_options, true).map_err(LlmOpError::JsonishError)?;

    let parse_ctx = ::bex_sap::deserializer::coercer::ParsingContext::new(sap.db());
    let target = sap
        .ty_resolved()
        .map_err(|err| parse_ctx.error_type_resolution(err))
        .map_err(LlmOpError::SapError)?;
    let parsed = ::bex_sap::sap_model::TyResolvedRef::coerce(&parse_ctx, target, &jsonish)
        .map_err(LlmOpError::SapError)?
        .ok_or_else(|| {
            LlmOpError::ParseResponseError("SAP parse returned no value when complete".to_string())
        })?;

    // === Convert back to baml ===
    Ok(::bex_sap::to_external::baml_value_to_external(
        &parsed,
        sap.db(),
    ))
}

pub fn execute_sap_parse_partial(
    json: &str,
    sap: &SapParseCache,
    _ctx: &::sys_types::SysOpContext,
) -> Result<Option<bex_external_types::BexExternalValue>, LlmOpError> {
    // === Jsonish ===
    let jsonish_options = ::bex_sap::jsonish::ParseOptions::default();
    let jsonish = ::bex_sap::jsonish::parse(json, jsonish_options, false)
        .map_err(LlmOpError::JsonishError)?;

    // === SAP parsing (use the streaming type for partial results) ===
    let parse_ctx = ::bex_sap::deserializer::coercer::ParsingContext::new(sap.db());
    let target = sap
        .stream_ty_resolved()
        .map_err(|err| parse_ctx.error_type_resolution(err))
        .map_err(LlmOpError::SapError)?;
    let parsed = ::bex_sap::sap_model::TyResolvedRef::coerce(&parse_ctx, target, &jsonish)
        .map_err(LlmOpError::SapError)?;
    // === Convert back to baml ===
    match parsed {
        Some(parsed) => {
            let converted = ::bex_sap::to_external::baml_value_to_external(&parsed, sap.db());
            Ok(Some(converted))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::LlmOpError;

    /// A parse failure must reach BAML as `LlmClient`, which is what
    /// `ai.errors.normalize` classifies into `ai.errors.ParseFailed`. Mapping
    /// it to a generic internal error instead would make a provider's bad
    /// answer look like an engine defect and escape the client error taxonomy.
    #[test]
    fn parse_response_error_maps_to_llm_client() {
        let mapped: ::sys_types::VmRustFnError =
            LlmOpError::ParseResponseError("bad json".to_string()).into();
        assert_eq!(
            mapped,
            ::sys_types::VmRustFnError::BamlError(::sys_types::VmBamlError::LlmClient {
                message: "bad json".to_string(),
            })
        );
    }

    /// A shape mismatch is the CALLER's problem, so it maps to
    /// `InvalidArgument` — a different `baml.errors.*` class, and catchable on
    /// its own.
    #[test]
    fn type_error_maps_to_invalid_argument() {
        let mapped: ::sys_types::VmRustFnError = LlmOpError::TypeError {
            expected: "string",
            actual: "int".to_string(),
        }
        .into();
        assert_eq!(
            mapped,
            ::sys_types::VmRustFnError::BamlError(::sys_types::VmBamlError::InvalidArgument {
                message: "expected string, got int".to_string(),
            })
        );
    }
}
