use crate::{
    ast::{self, WithIdentifier, WithName},
    DatamodelError, Diagnostics,
};

pub(crate) fn validate_attribute_name(ast_attr: &ast::Attribute, diagnostics: &mut Diagnostics) {
    validate_name("attribute", ast_attr.identifier(), diagnostics, false);
}

pub(crate) fn validate_generator_name(
    ast_gen: &ast::GeneratorConfig,
    diagnostics: &mut Diagnostics,
) {
    validate_name("generator", ast_gen.identifier(), diagnostics, false);
}

pub(crate) fn validate_client_name(ast_client: &ast::Client, diagnostics: &mut Diagnostics) {
    validate_name("client", ast_client.identifier(), diagnostics, true);
}

pub(crate) fn validate_variant_name(ast_variant: &ast::Variant, diagnostics: &mut Diagnostics) {
    validate_name("impl", ast_variant.identifier(), diagnostics, true);
}

pub(crate) fn validate_class_name(ast_class: &ast::Class, diagnostics: &mut Diagnostics) {
    validate_name("class", ast_class.identifier(), diagnostics, true);
}

pub(crate) fn validate_class_fiel_name(
    ast_class_field: &ast::Field,
    diagnostics: &mut Diagnostics,
) {
    validate_name(
        "class field",
        ast_class_field.identifier(),
        diagnostics,
        false,
    );
}

pub(crate) fn validate_function_name(ast_func: &ast::Function, diagnostics: &mut Diagnostics) {
    validate_name("function", ast_func.identifier(), diagnostics, true);
}

pub(crate) fn validate_enum_name(ast_enum: &ast::Enum, diagnostics: &mut Diagnostics) {
    validate_name("enum", ast_enum.identifier(), diagnostics, true);
    ast_enum.iter_values().for_each(|(_, val)| {
        validate_name("enum value", val.identifier(), diagnostics, true);
    })
}

pub(crate) fn validate_enum_value_name(
    ast_enum_value: &ast::EnumValue,
    diagnostics: &mut Diagnostics,
) {
    validate_name("enum value", ast_enum_value.identifier(), diagnostics, true);
}

fn validate_name(
    _type: &str,
    idn: &ast::Identifier,
    diagnostics: &mut Diagnostics,
    require_upper_case: bool,
) {
    let res = match idn {
        ast::Identifier::ENV(_, span) => Err(DatamodelError::new_name_error(
            _type,
            "env.* is reserved.",
            span.clone(),
        )),
        ast::Identifier::Ref(_, span) => Err(DatamodelError::new_name_error(
            _type,
            "Namespace imports (using '.') are not yet supported.",
            span.clone(),
        )),
        ast::Identifier::Primitive(t, span) => Err(DatamodelError::new_name_error(
            _type,
            &format!("{} is a primitive type.", idn.name()),
            span.clone(),
        )),
        ast::Identifier::Invalid(_, span) | ast::Identifier::String(_, span) => {
            Err(DatamodelError::new_name_error(
                _type,
                "Used invalid token. Only letters, numbers, and _. And must start with an uppercase letter.",
                span.clone(),
            ))
        }
        ast::Identifier::Local(val, span) => if require_upper_case && !val.chars().next().unwrap().is_uppercase() {
            Err(DatamodelError::new_name_error(
                _type,
                "Must start with an uppercase letter.",
                span.clone(),
            ))
        } else {
            if RESERVED_NAMES.contains(&val.as_str()) {
                Err(DatamodelError::new_name_error(
                    _type,
                    "This name is reserved.",
                    span.clone(),
                ))
            } else {
                Ok(())
            }
        },
    };

    match res {
        Ok(_) => {}
        Err(e) => diagnostics.push_error(e),
    }
}

// The source of the following list is from prisma-client-js. Any edit should be done in both places.
// https://github.com/prisma/prisma/blob/master/src/packages/client/src/generation/generateClient.ts#L443
const RESERVED_NAMES: &[&str] = &["BamlClient"];
