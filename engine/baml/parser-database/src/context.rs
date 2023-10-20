use std::collections::HashMap;

use crate::{
    ast, ast::WithName, interner::StringInterner, names::Names, types::Types, DatamodelError,
    Diagnostics, StringId,
};

use self::attributes::AttributesValidationState;

mod attributes;

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
    attributes: AttributesValidationState, // state machine for attribute validation
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
            attributes: Default::default(),
        }
    }

    pub(crate) fn push_attribute_validation_error(&mut self, message: &str, as_block: bool) {
        let attribute = self.current_attribute();

        let err = DatamodelError::new_attribute_validation_error(
            message,
            &format!(
                "{}{}",
                match as_block {
                    true => "@@",
                    _ => "@",
                },
                attribute.name()
            ),
            attribute.span.clone(),
        );
        self.push_error(err);
    }

    pub(super) fn push_error(&mut self, error: DatamodelError) {
        self.diagnostics.push_error(error)
    }

    /// All attribute validation should go through `visit_attributes()`. It lets
    /// us enforce some rules, for example that certain attributes should not be
    /// repeated, and make sure that _all_ attributes are visited during the
    /// validation process, emitting unknown attribute errors when it is not
    /// the case.
    ///
    /// - When you are done validating an attribute, you must call `discard_arguments()` or
    ///   `validate_visited_arguments()`. Otherwise, Context will helpfully panic.
    /// - When you are done validating an attribute set, you must call
    ///   `validate_visited_attributes()`. Otherwise, Context will helpfully panic.
    pub(super) fn visit_attributes(&mut self, ast_attributes: ast::AttributeContainer) {
        if self.attributes.attributes.is_some() || !self.attributes.unused_attributes.is_empty() {
            panic!(
                "`ctx.visit_attributes() called with {:?} while the Context is still validating previous attribute set on {:?}`",
                ast_attributes,
                self.attributes.attributes
            );
        }

        self.attributes.set_attributes(ast_attributes, self.ast);
    }

    /// Extract an attribute that can occur zero or more times. Example: @@index on models.
    ///
    /// Returns `true` as long as a next attribute is found.
    pub(crate) fn visit_repeated_attr(&mut self, name: &'static str) -> bool {
        let mut has_valid_attribute = false;

        while !has_valid_attribute {
            let first_attr = iter_attributes(self.attributes.attributes.as_ref(), self.ast)
                .filter(|(_, attr)| attr.name.name == name)
                .find(|(attr_id, _)| self.attributes.unused_attributes.contains(attr_id));
            let (attr_id, attr) = if let Some(first_attr) = first_attr {
                first_attr
            } else {
                break;
            };
            self.attributes.unused_attributes.remove(&attr_id);
            has_valid_attribute = self.set_attribute(attr_id, attr);
        }

        has_valid_attribute
    }

    /// Validate an _optional_ attribute that should occur only once. Returns whether the attribute
    /// is defined.
    #[must_use]
    pub(crate) fn visit_optional_single_attr(&mut self, name: &'static str) -> bool {
        let mut attrs = iter_attributes(self.attributes.attributes.as_ref(), self.ast)
            .filter(|(_, a)| a.name.name == name);
        let (first_idx, first) = match attrs.next() {
            Some(first) => first,
            None => return false,
        };
        let diagnostics = &mut self.diagnostics;

        if attrs.next().is_some() {
            for (idx, attr) in iter_attributes(self.attributes.attributes.as_ref(), self.ast)
                .filter(|(_, a)| a.name.name == name)
            {
                diagnostics.push_error(DatamodelError::new_duplicate_attribute_error(
                    &attr.name.name,
                    attr.span.clone(),
                ));
                assert!(self.attributes.unused_attributes.remove(&idx));
            }

            return false; // stop validation in this case
        }

        assert!(self.attributes.unused_attributes.remove(&first_idx));
        drop(attrs);
        self.set_attribute(first_idx, first)
    }

    /// Gets the argument with the given name in the current attribute, or if it is not found, the
    /// first unnamed argument.
    ///
    /// Use this to implement unnamed argument behavior.
    pub(crate) fn visit_default_arg_with_idx(
        &mut self,
        name: &'static str,
    ) -> Result<(usize, &'db ast::Expression), DatamodelError> {
        let name_s = self.interner.intern(name);
        match (
            self.attributes.args.remove(&Some(name_s)),
            self.attributes.args.remove(&None),
        ) {
            (Some(arg_idx), None) | (None, Some(arg_idx)) => {
                let arg = self.arg_at(arg_idx);
                Ok((arg_idx, &arg.value))
            }
            (Some(arg_idx), Some(_)) => {
                let arg = self.arg_at(arg_idx);
                Err(DatamodelError::new_duplicate_default_argument_error(
                    name,
                    arg.span.clone(),
                ))
            }
            (None, None) => Err(DatamodelError::new_argument_not_found_error(
                name,
                self.current_attribute().span.clone(),
            )),
        }
    }

    /// This must be called at the end of arguments validation. It will report errors for each argument that was not used by the validators. The Drop impl will helpfully panic
    /// otherwise.
    pub(crate) fn validate_visited_arguments(&mut self) {
        let attr = if let Some(attrid) = self.attributes.attribute {
            &self.ast[attrid]
        } else {
            panic!("State error: missing attribute in validate_visited_arguments.")
        };

        let diagnostics = &mut self.diagnostics;
        for arg_idx in self.attributes.args.values() {
            let arg = &attr.arguments.arguments[*arg_idx];
            diagnostics.push_error(DatamodelError::new_unused_argument_error(arg.span.clone()));
        }

        self.discard_arguments();
    }

    /// Counterpart to visit_attributes(). This must be called at the end of the validation of the
    /// attribute set. The Drop impl will helpfully panic otherwise.
    pub(crate) fn validate_visited_attributes(&mut self) {
        if !self.attributes.args.is_empty() || self.attributes.attribute.is_some() {
            panic!("State error: validate_visited_attributes() when an attribute is still under validation.");
        }

        let diagnostics = &mut self.diagnostics;
        for attribute_id in &self.attributes.unused_attributes {
            let attribute = &self.ast[*attribute_id];
            diagnostics.push_error(DatamodelError::new_attribute_not_known_error(
                &attribute.name.name,
                attribute.span.clone(),
            ))
        }

        self.attributes.attributes = None;
        self.attributes.unused_attributes.clear();
    }

    // Private methods start here.

    /// Return the attribute currently being validated. Panics if the context is not in the right
    /// state.
    #[track_caller]
    pub(crate) fn current_attribute(&self) -> &'db ast::Attribute {
        let id = self.attributes.attribute.unwrap();
        &self.ast[id]
    }

    fn arg_at(&self, idx: usize) -> &'db ast::Argument {
        &self.current_attribute().arguments.arguments[idx]
    }

    /// Discard arguments without validation.
    pub(crate) fn discard_arguments(&mut self) {
        self.attributes.attribute = None;
        self.attributes.args.clear();
    }

    /// Starts validating the arguments for an attribute, checking for duplicate arguments in the
    /// process. Returns whether the attribute is valid enough to be usable.
    fn set_attribute(
        &mut self,
        attribute_id: ast::AttributeId,
        attribute: &'db ast::Attribute,
    ) -> bool {
        if self.attributes.attribute.is_some() || !self.attributes.args.is_empty() {
            panic!("State error: we cannot start validating new arguments before `validate_visited_arguments()` or `discard_arguments()` has been called.\n{:#?}", self.attributes);
        }

        let mut is_reasonably_valid = true;

        let arguments = &attribute.arguments;

        self.attributes.attribute = Some(attribute_id);
        self.attributes.args.clear();
        self.attributes.args.reserve(arguments.arguments.len());
        let mut unnamed_arguments = Vec::new();

        for (arg_idx, arg) in arguments.arguments.iter().enumerate() {
            let arg_name = arg
                .name
                .as_ref()
                .map(|name| self.interner.intern(&name.name));
            if let Some(existing_argument) = self.attributes.args.insert(arg_name, arg_idx) {
                if arg.is_unnamed() {
                    if unnamed_arguments.is_empty() {
                        let existing_arg_value =
                            &attribute.arguments.arguments[existing_argument].value;
                        unnamed_arguments.push(existing_arg_value.to_string())
                    }

                    unnamed_arguments.push(arg.value.to_string())
                } else {
                    self.push_error(DatamodelError::new_duplicate_argument_error(
                        &arg.name.as_ref().unwrap().name,
                        arg.span.clone(),
                    ));
                }
            }
        }

        if !unnamed_arguments.is_empty() {
            self.push_attribute_validation_error(
                &format!("You provided multiple unnamed arguments. This is not possible. Did you forget the brackets? Did you mean `[{}]`?", unnamed_arguments.join(", ")),
                false,
                )
        }

        true
    }
}

// Implementation detail. Used for arguments validation.
fn iter_attributes<'a, 'ast: 'a>(
    attrs: Option<&'a ast::AttributeContainer>,
    ast: &'ast ast::SchemaAst,
) -> impl Iterator<Item = (ast::AttributeId, &'ast ast::Attribute)> + 'a {
    attrs
        .into_iter()
        .flat_map(move |container| ast[*container].iter().enumerate().map(|a| (a, *container)))
        .map(|((idx, attr), container)| (ast::AttributeId::new_in_container(container, idx), attr))
}

impl std::ops::Index<StringId> for Context<'_> {
    type Output = str;

    fn index(&self, index: StringId) -> &Self::Output {
        self.interner.get(index).unwrap()
    }
}
