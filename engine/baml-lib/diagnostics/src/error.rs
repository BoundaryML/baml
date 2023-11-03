use colored::{ColoredString, Colorize};

use crate::{
    pretty_print::{pretty_print, DiagnosticColorer},
    Span,
};
use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct DatamodelError {
    span: Span,
    message: Cow<'static, str>,
}

impl DatamodelError {
    pub(crate) fn new(message: impl Into<Cow<'static, str>>, span: Span) -> Self {
        let message = message.into();
        DatamodelError { message, span }
    }

    pub fn new_static(message: &'static str, span: Span) -> Self {
        Self::new(message, span)
    }

    pub fn new_literal_parser_error(
        literal_type: &str,
        raw_value: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("\"{raw_value}\" is not a valid value for {literal_type}."),
            span,
        )
    }

    pub fn new_argument_not_found_error(argument_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Argument \"{argument_name}\" is missing."), span)
    }

    pub fn new_argument_count_mismatch_error(
        function_name: &str,
        required_count: usize,
        given_count: usize,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Function \"{function_name}\" takes {required_count} arguments, but received {given_count}.");
        Self::new(msg, span)
    }

    pub fn new_attribute_argument_not_found_error(
        argument_name: &str,
        attribute_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("Argument \"{argument_name}\" is missing in attribute \"@{attribute_name}\"."),
            span,
        )
    }

    pub fn new_generator_argument_not_found_error(
        argument_name: &str,
        generator_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!(
                "Argument \"{argument_name}\" is missing in generator block \"{generator_name}\"."
            ),
            span,
        )
    }

    pub fn new_attribute_validation_error(
        message: &str,
        attribute_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("Error parsing attribute \"{attribute_name}\": {message}"),
            span,
        )
    }

    pub fn new_duplicate_attribute_error(attribute_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Attribute \"@{attribute_name}\" can only be defined once.");
        Self::new(msg, span)
    }

    pub fn new_incompatible_native_type(
        native_type: &str,
        field_type: &str,
        expected_types: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Native type {native_type} is not compatible with declared field type {field_type}, expected field type {expected_types}.",
        );
        Self::new(msg, span)
    }

    pub fn new_invalid_native_type_argument(
        native_type: &str,
        got: &str,
        expected: &str,
        span: Span,
    ) -> DatamodelError {
        let msg =
            format!("Invalid argument for type {native_type}: {got}. Allowed values: {expected}.");
        Self::new(msg, span)
    }

    pub fn new_invalid_prefix_for_native_types(
        given_prefix: &str,
        expected_prefix: &str,
        suggestion: &str,
        span: Span,
    ) -> DatamodelError {
        let msg =  format!("The prefix {given_prefix} is invalid. It must be equal to the name of an existing datasource e.g. {expected_prefix}. Did you mean to use {suggestion}?");
        DatamodelError::new(msg, span)
    }

    pub fn new_native_types_not_supported(connector_name: String, span: Span) -> DatamodelError {
        let msg = format!("Native types are not supported with {connector_name} connector");
        Self::new(msg, span)
    }

    pub fn new_reserved_scalar_type_error(type_name: &str, span: Span) -> DatamodelError {
        let msg = format!("\"{type_name}\" is a reserved scalar type name and cannot be used.");
        Self::new(msg, span)
    }

    pub fn new_duplicate_enum_database_name_error(span: Span) -> DatamodelError {
        let msg = "An enum with the same database name is already defined.";
        Self::new(msg, span)
    }

    pub fn new_duplicate_model_database_name_error(
        model_database_name: &str,
        existing_model_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("The model with database name \"{model_database_name}\" could not be defined because another model or view with this name exists: \"{existing_model_name}\"");
        Self::new(msg, span)
    }

    pub fn new_duplicate_view_database_name_error(
        model_database_name: &str,
        existing_model_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("The view with database name \"{model_database_name}\" could not be defined because another model or view with this name exists: \"{existing_model_name}\"");
        Self::new(msg, span)
    }

    pub fn new_duplicate_top_error(
        name: &str,
        top_type: &str,
        existing_top_type: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "The {top_type} \"{name}\" cannot be defined because a {existing_top_type} with that name already exists.",
        );
        Self::new(msg, span)
    }

    pub fn new_duplicate_config_key_error(
        conf_block_name: &str,
        key_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Key \"{key_name}\" is already defined in {conf_block_name}.");
        Self::new(msg, span)
    }

    pub fn new_duplicate_argument_error(arg_name: &str, span: Span) -> DatamodelError {
        Self::new(
            format!("Argument \"{arg_name}\" is already specified."),
            span,
        )
    }

    pub fn new_unused_argument_error(span: Span) -> DatamodelError {
        Self::new("No such argument.", span)
    }

    pub fn new_duplicate_default_argument_error(arg_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Argument \"{arg_name}\" is already specified as unnamed argument.");
        Self::new(msg, span)
    }

    pub fn new_duplicate_enum_value_error(
        enum_name: &str,
        value_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Value \"{value_name}\" is already defined on enum \"{enum_name}\".",);
        Self::new(msg, span)
    }

    pub fn new_composite_type_duplicate_field_error(
        type_name: &str,
        field_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Field \"{}\" is already defined on {} \"{}\".",
            field_name, "composite type", type_name
        );
        Self::new(msg, span)
    }

    pub fn new_duplicate_field_error(
        model_name: &str,
        field_name: &str,
        container: &'static str,
        span: Span,
    ) -> DatamodelError {
        let msg =
            format!("Field \"{field_name}\" is already defined on {container} \"{model_name}\".",);
        Self::new(msg, span)
    }

    pub fn new_scalar_list_fields_are_not_supported(
        container: &str,
        container_name: &str,
        field_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Field \"{field_name}\" in {container} \"{container_name}\" can't be a list. The current connector does not support lists of primitive types.");
        Self::new(msg, span)
    }

    pub fn new_model_validation_error(
        message: &str,
        block_type: &'static str,
        model_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("Error validating {block_type} \"{model_name}\": {message}"),
            span,
        )
    }

    pub fn new_name_error(_type: &str, message: &str, span: Span) -> DatamodelError {
        Self::new(format!("Invalid name for `{_type}`: {message}"), span)
    }

    pub fn new_enum_validation_error(message: &str, enum_name: &str, span: Span) -> DatamodelError {
        Self::new(
            format!("Error validating enum `{enum_name}`: {message}"),
            span,
        )
    }

    pub fn new_composite_type_field_validation_error(
        message: &str,
        composite_type_name: &str,
        field: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Error validating field `{}` in {} `{}`: {}",
            field, "composite type", composite_type_name, message
        );
        Self::new(msg, span)
    }

    pub fn new_field_validation_error(
        message: &str,
        container_type: &str,
        container_name: &str,
        field: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Error validating field `{field}` in {container_type} `{container_name}`: {message}",
        );
        Self::new(msg, span)
    }

    pub fn new_source_validation_error(message: &str, source: &str, span: Span) -> DatamodelError {
        Self::new(
            format!("Error validating datasource `{source}`: {message}"),
            span,
        )
    }

    pub fn new_validation_error(message: &str, span: Span) -> DatamodelError {
        Self::new(format!("Error validating: {message}"), span)
    }

    pub fn new_legacy_parser_error(
        message: impl Into<Cow<'static, str>>,
        span: Span,
    ) -> DatamodelError {
        Self::new(message.into(), span)
    }

    pub fn new_optional_argument_count_mismatch(
        native_type: &str,
        optional_count: usize,
        given_count: usize,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Native type {native_type} takes {optional_count} optional arguments, but received {given_count}.",
        );

        DatamodelError::new(msg, span)
    }

    pub fn new_parser_error(expected_str: String, span: Span) -> DatamodelError {
        Self::new(
            format!("Unexpected token. Expected one of: {expected_str}"),
            span,
        )
    }

    pub fn new_functional_evaluation_error(
        message: impl Into<Cow<'static, str>>,
        span: Span,
    ) -> DatamodelError {
        Self::new(message.into(), span)
    }

    pub fn type_not_used_in_prompt_error(
        is_enum: bool,
        type_exists: bool,
        function_name: &str,
        type_name: &str,
        names: Vec<String>,
        span: Span,
    ) -> DatamodelError {
        // Filter names that are within the threshold
        let close_names = {
            // Calculate OSA distances and sort names by distance
            let mut distances = names
                .iter()
                .map(|n| {
                    (
                        strsim::osa_distance(&n.to_lowercase(), &type_name.to_lowercase()),
                        n.to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            if !is_enum {
                distances.push((
                    strsim::osa_distance("output", &type_name.to_lowercase()),
                    "output".to_string(),
                ));
            }
            distances.sort_by_key(|k| k.0);

            // Set a threshold for "closeness"
            let threshold = 10; // for example, you can adjust this based on your needs

            distances
                .iter()
                .filter(|&&(dist, _)| dist <= threshold)
                .map(|(_, name)| name.to_owned())
                .collect::<Vec<_>>()
        };

        let prefix = if type_exists {
            format!(
                "{} `{}` is not used in in the output of function `{}`.",
                is_enum.then(|| "Enum").unwrap_or("Type"),
                type_name,
                function_name
            )
        } else {
            format!(
                "{} `{}` does not exist.",
                is_enum.then(|| "Enum").unwrap_or("Type"),
                type_name,
            )
        };

        let suggestions = if names.is_empty() {
            if is_enum {
                format!(" No Enums are used in the output of this function.",)
            } else {
                format!(" Did you mean `output`?",)
            }
        } else if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            "".to_string()
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(" Did you mean `{}`?", close_names[0])
        } else {
            // If there are multiple close names, suggest them all
            format!(
                " Did you mean one of these: `{}`?",
                close_names
                    .iter()
                    .take(3)
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("`, `")
            )
        };

        Self::new(format!("{}{}", prefix, suggestions), span)
    }

    pub fn new_type_not_found_error(
        type_name: &str,
        names: Vec<String>,
        span: Span,
    ) -> DatamodelError {
        // Calculate OSA distances and sort names by distance
        let mut distances = names
            .iter()
            .map(|n| {
                (
                    strsim::osa_distance(&n.to_lowercase(), &type_name.to_lowercase()),
                    n.to_owned(),
                )
            })
            .collect::<Vec<_>>();
        distances.sort_by_key(|k| k.0);

        // Set a threshold for "closeness"
        let threshold = 2; // for example, you can adjust this based on your needs

        // Filter names that are within the threshold
        let close_names = distances
            .iter()
            .filter(|&&(dist, _)| dist <= threshold)
            .map(|(_, name)| name.to_owned())
            .collect::<Vec<_>>();

        let msg = if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            format!("Type `{}` does not exist.", type_name)
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(
                "Type `{}` does not exist. Did you mean `{}`?",
                type_name, close_names[0]
            )
        } else {
            // If there are multiple close names, suggest them all
            let suggestions = close_names.join("`, `");
            format!(
                "Type `{}` does not exist. Did you mean one of these: `{}`?",
                type_name, suggestions
            )
        };

        Self::new(msg, span)
    }

    pub fn new_attribute_not_known_error(attribute_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Attribute not known: \"@{attribute_name}\"."), span)
    }

    pub fn new_property_not_known_error(property_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Property not known: \"{property_name}\"."), span)
    }

    pub fn new_argument_not_known_error(property_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Argument not known: \"{property_name}\"."), span)
    }

    pub fn new_value_parser_error(expected_type: &str, raw: &str, span: Span) -> DatamodelError {
        let msg = format!("Expected {expected_type}, but found {raw}.");
        Self::new(msg, span)
    }

    pub fn new_type_mismatch_error(
        expected_type: &str,
        received_type: &str,
        raw: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Expected a {expected_type} value, but received {received_type} value `{raw}`."
        );
        Self::new(msg, span)
    }

    pub fn new_missing_required_property_error(
        property_name: &str,
        object_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg =
            format!("The required property \"{property_name}\" on \"{object_name}\" is missing.",);
        Self::new(msg, span)
    }

    pub fn new_config_property_missing_value_error(
        property_name: &str,
        config_name: &str,
        config_kind: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Property {property_name} in {config_kind} {config_name} needs to be assigned a value"
        );
        Self::new(msg, span)
    }

    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn pretty_print(&self, f: &mut dyn std::io::Write) -> std::io::Result<()> {
        pretty_print(
            f,
            self.span(),
            self.message.as_ref(),
            &DatamodelErrorColorer {},
        )
    }
}

struct DatamodelErrorColorer {}

impl DiagnosticColorer for DatamodelErrorColorer {
    fn title(&self) -> &'static str {
        "error"
    }

    fn primary_color(&self, token: &'_ str) -> ColoredString {
        token.bright_red()
    }
}
