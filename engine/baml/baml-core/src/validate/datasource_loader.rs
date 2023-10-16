use crate::{
    ast::{self, SourceConfig, Span},
    configuration::StringFromEnvVar,
    datamodel_connector::RelationMode,
    internal_baml_diagnostics::{DatamodelError, Diagnostics},
    Datasource,
};
use internal_baml_diagnostics::DatamodelWarning;
use internal_baml_parser_database::{
    ast::{Expression, WithDocumentation},
    coerce, coerce_array, coerce_opt,
};
use std::{borrow::Cow, collections::HashMap};

const PREVIEW_FEATURES_KEY: &str = "previewFeatures";
const SCHEMAS_KEY: &str = "schemas";
const SHADOW_DATABASE_URL_KEY: &str = "shadowDatabaseUrl";
const URL_KEY: &str = "url";
const DIRECT_URL_KEY: &str = "directUrl";
const PROVIDER_KEY: &str = "provider";

/// Loads all datasources from the provided schema AST.
/// - `ignore_datasource_urls`: datasource URLs are not parsed. They are replaced with dummy values.
/// - `datasource_url_overrides`: datasource URLs are not parsed and overridden with the provided ones.
pub(crate) fn load_datasources_from_ast(
    ast_schema: &ast::SchemaAst,
    diagnostics: &mut Diagnostics,
    connectors: crate::ConnectorRegistry,
) -> Vec<Datasource> {
    let mut sources = Vec::new();

    for src in ast_schema.sources() {
        if let Some(source) = lift_datasource(src, diagnostics, connectors) {
            sources.push(source)
        }
    }

    if sources.len() > 1 {
        for src in ast_schema.sources() {
            diagnostics.push_error(DatamodelError::new_source_validation_error(
                "You defined more than one datasource. This is not allowed yet because support for multiple databases has not been implemented yet.",
                &src.name.name,
                src.span,
            ));
        }
    }

    sources
}

fn lift_datasource(
    ast_source: &ast::SourceConfig,
    diagnostics: &mut Diagnostics,
    connectors: crate::ConnectorRegistry,
) -> Option<Datasource> {
    let source_name = ast_source.name.name.as_str();
    let mut args: HashMap<_, (_, &Expression)> = ast_source
        .properties
        .iter()
        .map(|arg| match &arg.value {
            Some(expr) => Some((arg.name.name.as_str(), (arg.span, expr))),
            None => {
                diagnostics.push_error(DatamodelError::new_config_property_missing_value_error(
                    &arg.name.name,
                    source_name,
                    "datasource",
                    ast_source.span,
                ));
                None
            }
        })
        .collect::<Option<HashMap<_, (_, _)>>>()?;

    let (provider, provider_arg) = match args.remove(PROVIDER_KEY) {
        Some((_span, provider_arg)) => {
            if provider_arg.is_env_expression() {
                let msg = Cow::Borrowed(
                    "A datasource must not use the env() function in the provider argument.",
                );
                diagnostics.push_error(DatamodelError::new_functional_evaluation_error(
                    msg,
                    ast_source.span,
                ));
                return None;
            }

            let provider = match coerce_opt::string(provider_arg) {
                Some("") => {
                    diagnostics.push_error(DatamodelError::new_source_validation_error(
                        "The provider argument in a datasource must not be empty",
                        source_name,
                        provider_arg.span(),
                    ));
                    return None;
                }
                None => {
                    diagnostics.push_error(DatamodelError::new_source_validation_error(
                        "The provider argument in a datasource must be a string literal",
                        source_name,
                        provider_arg.span(),
                    ));
                    return None;
                }
                Some(provider) => provider,
            };

            (provider, provider_arg)
        }

        None => {
            diagnostics.push_error(DatamodelError::new_source_argument_not_found_error(
                "provider",
                source_name,
                ast_source.span,
            ));
            return None;
        }
    };

    let active_connector: &'static dyn crate::datamodel_connector::Connector =
        match connectors.iter().find(|c| c.is_provider(provider)) {
            Some(c) => *c,
            None => {
                diagnostics.push_error(DatamodelError::new_datasource_provider_not_known_error(
                    provider,
                    provider_arg.span(),
                ));

                return None;
            }
        };

    let relation_mode = get_relation_mode(&mut args, ast_source, diagnostics, active_connector);

    let connector_data = active_connector.parse_datasource_properties(&mut args, diagnostics);

    let (url, url_span) = match args.remove(URL_KEY) {
        Some((_span, url_arg)) => (
            StringFromEnvVar::coerce(url_arg, diagnostics)?,
            url_arg.span(),
        ),

        None => {
            diagnostics.push_error(DatamodelError::new_source_argument_not_found_error(
                URL_KEY,
                source_name,
                ast_source.span,
            ));

            return None;
        }
    };

    let shadow_database_url = match args.remove(SHADOW_DATABASE_URL_KEY) {
        Some((_span, shadow_db_url_arg)) => {
            match StringFromEnvVar::coerce(shadow_db_url_arg, diagnostics) {
                Some(shadow_db_url) => Some(shadow_db_url)
                    .filter(|s| {
                        !s.as_literal()
                            .map(|literal| literal.is_empty())
                            .unwrap_or(false)
                    })
                    .map(|url| (url, shadow_db_url_arg.span())),
                None => None,
            }
        }

        _ => None,
    };

    let (direct_url, direct_url_span) = match args.remove(DIRECT_URL_KEY) {
        Some((_, direct_url)) => (
            StringFromEnvVar::coerce(direct_url, diagnostics),
            Some(direct_url.span()),
        ),

        None => (None, None),
    };

    preview_features_guardrail(&mut args, diagnostics);

    let documentation = ast_source.documentation().map(String::from);

    let (schemas, schemas_span) = match args.remove(SCHEMAS_KEY) {
        Some((_span, schemas)) => coerce_array(schemas, &coerce::string_with_span, diagnostics)
            .map(|b| (b, schemas.span()))
            .and_then(|(mut schemas, span)| {
                if schemas.is_empty() {
                    diagnostics.push_error(DatamodelError::new_schemas_array_empty_error(span));

                    return None;
                }

                schemas.sort_by(|(a, _), (b, _)| a.cmp(b));

                for pair in schemas.windows(2) {
                    if pair[0].0 == pair[1].0 {
                        diagnostics.push_error(DatamodelError::new_static(
                            "Duplicated schema names are not allowed",
                            pair[0].1,
                        ))
                    }
                }

                Some((schemas, Some(span)))
            })
            .unwrap_or_default(),

        None => Default::default(),
    };

    for (name, (span, _)) in args.into_iter() {
        diagnostics.push_error(DatamodelError::new_property_not_known_error(name, span));
    }

    Some(Datasource {
        namespaces: schemas
            .into_iter()
            .map(|(s, span)| (s.to_owned(), span))
            .collect(),
        schemas_span,
        name: source_name.to_owned(),
        provider: provider.to_owned(),
        active_provider: active_connector.provider_name(),
        url,
        url_span,
        direct_url,
        direct_url_span,
        documentation,
        active_connector,
        shadow_database_url,
        relation_mode,
        connector_data,
    })
}

/// Returns the relation mode for the datasource, validating against invalid relation mode settings and
/// the deprecated `referentialIntegrity` attribute.
fn get_relation_mode(
    args: &mut HashMap<&str, (Span, &ast::Expression)>,
    source: &SourceConfig,
    diagnostics: &mut Diagnostics,
    connector: &'static dyn crate::datamodel_connector::Connector,
) -> Option<RelationMode> {
    // check for deprecated `referentialIntegrity` attribute.
    if let Some((span, _)) = args.get("referentialIntegrity") {
        diagnostics.push_warning(
            DatamodelWarning::new_referential_integrity_attr_deprecation_warning(*span),
        );
    }

    // figure out which attribute is used for the `relationMode` feature
    match (
        args.remove("relationMode"),
        args.remove("referentialIntegrity"),
    ) {
        (None, None) => None,
        (Some(_), Some((span, _))) => {
            // both possible attributes are used, which is invalid
            diagnostics.push_error(
                DatamodelError::new_referential_integrity_and_relation_mode_cooccur_error(span),
            );
            None
        }
        (Some((_span, rm)), None) | (None, Some((_span, rm))) => {
            // either `relationMode` or `referentialIntegrity` is used, which is valid
            let relation_mode = match coerce::string(rm, diagnostics)? {
                "prisma" => RelationMode::Prisma,
                "foreignKeys" => RelationMode::ForeignKeys,
                other => {
                    let message = format!(
                        "Invalid relation mode setting: \"{other}\". Supported values: \"prisma\", \"foreignKeys\"",
                    );
                    let error = DatamodelError::new_source_validation_error(
                        &message,
                        "relationMode",
                        source.span,
                    );
                    diagnostics.push_error(error);
                    return None;
                }
            };

            if !connector
                .allowed_relation_mode_settings()
                .contains(relation_mode)
            {
                let supported_values = connector
                    .allowed_relation_mode_settings()
                    .iter()
                    .map(|v| format!(r#""{v}""#))
                    .collect::<Vec<_>>()
                    .join(", ");

                let message = format!(
                    "Invalid relation mode setting: \"{relation_mode}\". Supported values: {supported_values}",
                );
                let error = DatamodelError::new_source_validation_error(
                    &message,
                    "relationMode",
                    rm.span(),
                );
                diagnostics.push_error(error);
            }

            Some(relation_mode)
        }
    }
}

fn preview_features_guardrail(
    args: &mut HashMap<&str, (Span, &ast::Expression)>,
    diagnostics: &mut Diagnostics,
) {
    if let Some((span, _)) = args.remove(PREVIEW_FEATURES_KEY) {
        let msg = "Preview features are only supported in the generator block. Please move this field to the generator block.";
        diagnostics.push_error(DatamodelError::new_static(msg, span));
    }
}
