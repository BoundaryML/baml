use std::collections::HashMap;

use crate::{
    ast, interner::StringInterner, names::Names, types::Types, DatamodelError, Diagnostics,
    StringId,
};

/// Validation context. This is an implementation detail of ParserDatabase. It
/// contains the database itself, as well as context that is discarded after
/// validation.
///
/// ## Attribute Validation
///
/// The Context also acts as a state machine for attribute validation. The goal is to avoid manual
/// work validating things that are valid for every attribute set, and every argument set inside an
/// attribute: multiple unnamed arguments are not valid, attributes we do not use in parser-database
/// are not valid, multiple arguments with the same name are not valid, etc.
///
/// See `visit_attributes()`.
pub(crate) struct Context<'db> {
    pub(crate) ast: &'db ast::SchemaAst,
    pub(crate) interner: &'db mut StringInterner,
    pub(crate) names: &'db mut Names,
    pub(crate) types: &'db mut Types,
    pub(crate) diagnostics: &'db mut Diagnostics,
    // attributes: AttributesValidationState, // state machine for attribute validation

    // @map'ed names indexes. These are not in the db because they are only used for validation.
    pub(super) mapped_enum_names: HashMap<StringId, ast::EnumId>,
    pub(super) mapped_enum_value_names: HashMap<(ast::EnumId, StringId), u32>,
}

impl<'db> Context<'db> {
    pub(super) fn new(
        ast: &'db ast::SchemaAst,
        interner: &'db mut StringInterner,
        names: &'db mut Names,
        types: &'db mut Types,
        diagnostics: &'db mut Diagnostics,
    ) -> Self {
        Context {
            ast,
            interner,
            names,
            types,
            diagnostics,
            // attributes: AttributesValidationState::default(),

            // mapped_model_scalar_field_names: Default::default(),
            mapped_enum_names: Default::default(),
            mapped_enum_value_names: Default::default(),
            // mapped_composite_type_names: Default::default(),
        }
    }

    pub(super) fn push_error(&mut self, error: DatamodelError) {
        self.diagnostics.push_error(error)
    }

    // Private methods start here.
}


impl std::ops::Index<StringId> for Context<'_> {
    type Output = str;

    fn index(&self, index: StringId) -> &Self::Output {
        self.interner.get(index).unwrap()
    }
}
