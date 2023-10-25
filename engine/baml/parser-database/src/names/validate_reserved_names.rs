use crate::{
    ast::{self, WithName, WithSpan},
    DatamodelError, Diagnostics,
};

/// Is this a valid type name for the Prisma Client API?
pub fn is_reserved_type_name(name: &str) -> bool {
    RESERVED_NAMES.contains(&name)
}

pub(crate) fn validate_class_name(
    ast_model: &ast::Class,
    block_type: &'static str,
    diagnostics: &mut Diagnostics,
) {
    if !is_reserved_type_name(ast_model.name()) {
        return;
    }

    diagnostics.push_error(DatamodelError::new_model_validation_error(
        &format!(
            "The {block_type} name `{}` is invalid. It is a reserved name. Please change it.",
            ast_model.name()
        ),
        "class",
        ast_model.name(),
        ast_model.span().clone(),
    ))
}

pub(crate) fn validate_function_name(
    ast_model: &ast::Function,
    block_type: &'static str,
    diagnostics: &mut Diagnostics,
) {
    if is_reserved_type_name(ast_model.name()) {
        diagnostics.push_error(DatamodelError::new_model_validation_error(
            &format!(
                "The {block_type} name `{}` is invalid. It is a reserved name. Please change it.",
                ast_model.name()
            ),
            "function",
            ast_model.name(),
            ast_model.span().clone(),
        ))
    }
}

pub(crate) fn validate_enum_name(ast_enum: &ast::Enum, diagnostics: &mut Diagnostics) {
    if is_reserved_type_name(&ast_enum.name.name) {
        diagnostics.push_error(DatamodelError::new_enum_validation_error(
            &format!(
              "The enum name `{}` is invalid. It is a reserved name. Please change it. Read more at https://www.prisma.io/docs/reference/tools-and-interfaces/prisma-schema/data-model#naming-enums",
              ast_enum.name()
            ),
            ast_enum.name(),
            ast_enum.span().clone(),
        ))
    }
}

// The source of the following list is from prisma-client-js. Any edit should be done in both places.
// https://github.com/prisma/prisma/blob/master/src/packages/client/src/generation/generateClient.ts#L443
const RESERVED_NAMES: &[&str] = &[
    "BamlClient",
    "Int",
    "String",
    "Bool",
    "Char",
    "Float",
    "Dict",
    "Union",
    "Optional",
    "List",
];
