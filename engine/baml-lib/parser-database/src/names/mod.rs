mod validate_reserved_names;

use baml_types::{BamlMap, FieldType};
use indexmap::map::IndexedEntry;
use internal_baml_ast::ast::{ConfigBlockProperty, Expression, Field, WithIdentifier};
use internal_baml_diagnostics::Span;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use validate_reserved_names::*;

use self::validate_reserved_names::{validate_enum_value_name, validate_function_name};
use crate::{
    ast::{self, TopId, WithAttributes, WithName, WithSpan},
    coerce, coerce_array, Context, DatamodelError, StaticType, StringId,
};

/// Resolved names for use in the validation process.
#[derive(Default, Clone)]
pub(super) struct Names {
    /// Models, enums, composite types and type aliases
    pub(super) tops: HashMap<StringId, TopId>,
    /// Generators have their own namespace.
    pub(super) generators: HashMap<StringId, TopId>,
    /// Tests have their own namespace. <function_name, <test_name, test_id>>
    pub(super) tests: HashMap<StringId, HashMap<StringId, Span>>,
    pub(super) model_fields: HashMap<(ast::TypeExpId, StringId), ast::FieldId>,
}

#[derive(Default)]
struct DuplicateNames {
    tops: BamlMap<StringId, indexmap::IndexSet<TopId>>,
    generators: BamlMap<StringId, indexmap::IndexSet<TopId>>,
    // test_name: vec<(function_name, function_name_span)>
    tests: BamlMap<StringId, Vec<(String, Span)>>,
    dynamic_types: BamlMap<StringId, indexmap::IndexSet<TopId>>,
}

impl DuplicateNames {
    fn to_errors(self, ctx: &mut Context<'_>) {
        for category in [self.tops, self.generators] {
            for (name, ids) in category {
                let ast_tops = ids.iter().map(|id| &ctx.ast[*id]).collect::<Vec<_>>();
                let others = ast_tops
                    .iter()
                    .map(|ast_top| (ast_top.get_type(), ast_top.identifier().span().clone()))
                    .collect::<Vec<_>>();
                for (idx, ast_top) in ast_tops.iter().enumerate() {
                    let others = others
                        .iter()
                        .enumerate()
                        .filter_map(|(i, data)| if i != idx { Some(data.clone()) } else { None })
                        .collect::<Vec<_>>();
                    ctx.push_error(DatamodelError::new_duplicate_top_error(
                        ast_top.name(),
                        ast_top.get_type(),
                        ast_top.identifier().span().clone(),
                        others,
                    ));
                }
            }
        }
        for (name, ids) in self.dynamic_types {
            let ast_tops = ids.iter().map(|id| &ctx.ast[*id]).collect::<Vec<_>>();
            let others = ast_tops
                .iter()
                .map(|ast_top| (ast_top.get_type(), ast_top.identifier().span().clone()))
                .collect::<Vec<_>>();
            for (idx, ast_top) in ast_tops.iter().enumerate() {
                let others = others
                    .iter()
                    .enumerate()
                    .filter_map(|(i, data)| if i != idx { Some(data.clone()) } else { None })
                    .collect::<Vec<_>>();
                ctx.push_error(DatamodelError::new_duplicate_top_error(
                    ast_top
                        .name()
                        .strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                        .unwrap(),
                    ast_top.get_type(),
                    ast_top.identifier().span().clone(),
                    others,
                ));
            }
        }

        for (test_name, tests) in self.tests {
            for (idx, (function_name, function_name_span)) in tests.iter().enumerate() {
                let others = tests
                    .iter()
                    .enumerate()
                    .filter_map(|(i, data)| if i != idx { Some(data.clone()) } else { None })
                    .collect::<Vec<_>>();
                let test_name = ctx.interner.get(test_name).unwrap();
                ctx.push_error(DatamodelError::new_duplicate_test_error(
                    function_name,
                    test_name,
                    function_name_span.clone(),
                    others.iter().map(|(_, span)| span.clone()).collect(),
                ));
            }
        }
    }
}

/// `resolve_names()` is responsible for populating `ParserDatabase.names` and
/// validating that there are no name collisions in the following namespaces:
///
/// - Model, enum and type alias names
/// - Generators
/// - Model fields for each model
pub(super) fn resolve_names(ctx: &mut Context<'_>) {
    let mut tmp_names: HashSet<&str> = HashSet::default(); // throwaway container for duplicate checking
    let mut names = Names::default();

    let mut duplicate_names = DuplicateNames::default();

    for (top_id, top) in ctx.ast.iter_tops() {
        assert_is_not_a_reserved_scalar_type(top.identifier(), ctx);

        let namespace = match (top_id, top) {
            (_, ast::Top::Enum(ast_enum)) => {
                tmp_names.clear();
                validate_enum_name(ast_enum, ctx.diagnostics);
                validate_attribute_identifiers(ast_enum, ctx);

                for value in &ast_enum.fields {
                    validate_enum_value_name(value, ctx.diagnostics);

                    validate_attribute_identifiers(value, ctx);

                    if !tmp_names.insert(value.name()) {
                        ctx.push_error(DatamodelError::new_duplicate_enum_value_error(
                            ast_enum.name.name(),
                            value.name(),
                            value.span().clone(),
                        ))
                    }
                }

                Some(either::Left(&mut names.tops))
            }

            (ast::TopId::Class(model_id), ast::Top::Class(ast_class)) => {
                validate_class_name(ast_class, ctx.diagnostics);
                validate_attribute_identifiers(ast_class, ctx);

                for (field_id, field) in ast_class.iter_fields() {
                    validate_class_field_name(field, ctx.diagnostics);
                    validate_attribute_identifiers(field, ctx);

                    let field_name_id = ctx.interner.intern(field.name());
                    if names
                        .model_fields
                        .insert((model_id, field_name_id), field_id)
                        .is_some()
                    {
                        ctx.push_error(DatamodelError::new_duplicate_field_error(
                            ast_class.identifier().name(),
                            field.name(),
                            "class",
                            field.identifier().span().clone(),
                        ))
                    }
                }

                Some(either::Left(&mut names.tops))
            }
            (_, ast::Top::Class(_)) => {
                unreachable!("Encountered impossible class declaration during parsing")
            }

            (ast::TopId::TypeAlias(_), ast::Top::TypeAlias(type_alias)) => {
                validate_type_alias_name(type_alias, ctx.diagnostics);

                ctx.interner.intern(type_alias.name());

                Some(either::Left(&mut names.tops))
            }

            (_, ast::Top::TypeAlias(_)) => {
                unreachable!("Encountered impossible type alias declaration during parsing")
            }

            (ast::TopId::TemplateString(_), ast::Top::TemplateString(template_string)) => {
                validate_template_string_name(template_string, ctx.diagnostics);
                validate_attribute_identifiers(template_string, ctx);

                Some(either::Left(&mut names.tops))
            }
            (_, ast::Top::TemplateString(_)) => {
                unreachable!("Encountered impossible template_string declaration during parsing")
            }

            (ast::TopId::Function(_function_id), ast::Top::Function(ast_function)) => {
                validate_function_name(ast_function, ctx.diagnostics);
                validate_attribute_identifiers(ast_function, ctx);

                Some(either::Left(&mut names.tops))
            }
            (_, ast::Top::Function(_)) => {
                unreachable!("Encountered impossible function declaration during parsing")
            }

            (ast::TopId::Client(_), ast::Top::Client(ast_client)) => {
                validate_client_name(ast_client, ctx.diagnostics);
                validate_attribute_identifiers(ast_client, ctx);

                ctx.interner.intern(ast_client.identifier().name());

                Some(either::Left(&mut names.tops))
            }
            (_, ast::Top::Client(_)) => {
                unreachable!("Encountered impossible client declaration during parsing")
            }

            (ast::TopId::RetryPolicy(_), ast::Top::RetryPolicy(ast_retry_policy)) => {
                validate_retry(ast_retry_policy, ctx.diagnostics);
                validate_attribute_identifiers(ast_retry_policy, ctx);

                ctx.interner.intern(ast_retry_policy.identifier().name());

                Some(either::Left(&mut names.tops))
            }
            (_, ast::Top::RetryPolicy(_)) => {
                unreachable!("Encountered impossible retry_policy declaration during parsing")
            }

            (_, ast::Top::Generator(generator)) => {
                validate_generator_name(generator, ctx.diagnostics);
                check_for_duplicate_properties(top, generator.fields(), &mut tmp_names, ctx);
                Some(either::Left(&mut names.generators))
            }

            (ast::TopId::TestCase(testcase_id), ast::Top::TestCase(testcase)) => {
                validate_test(testcase, ctx.diagnostics);
                check_for_duplicate_properties(top, testcase.fields(), &mut tmp_names, ctx);

                // TODO: I think we should do this later after all parsing, as duplication
                // would work best as a validation error with walkers.
                let function_ids = testcase
                    .iter_fields()
                    .find(|f| f.1.name() == "functions")
                    .and_then(|f| match f.1.expr {
                        Some(ref v) => coerce_array(v, &coerce::string_with_span, ctx.diagnostics),
                        None => None,
                    });

                match function_ids {
                    Some(f) => Some(either::Right(f)),
                    None => {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "Test case must have a functions field",
                            testcase.identifier().span().clone(),
                        ));
                        None
                    }
                }
            }
            _ => None,
        };

        match namespace {
            Some(either::Left(namespace)) => {
                insert_name(top_id, top, &mut duplicate_names, namespace, ctx);
            }
            Some(either::Right(test_functions)) => {
                for (func_name, func_name_span) in test_functions {
                    let func_id = ctx.interner.intern(func_name);
                    let namespace = names.tests.entry(func_id).or_insert_with(HashMap::default);
                    let test_name = ctx.interner.intern(top.name());
                    if let Some(existing) = namespace.insert(test_name, func_name_span.clone()) {
                        duplicate_names
                            .tests
                            .entry(test_name)
                            .or_insert_with(Vec::default)
                            .push((func_name.to_string(), func_name_span.clone()));
                        duplicate_names
                            .tests
                            .entry(test_name)
                            .or_insert_with(Vec::default)
                            .push((func_name.to_string(), existing));
                    }
                }
            }
            None => {}
        }
    }

    duplicate_names.to_errors(ctx);
    let _ = std::mem::replace(ctx.names, names);
}

fn insert_name(
    top_id: TopId,
    top: &ast::Top,
    duplicate_names: &mut DuplicateNames,
    namespace: &mut HashMap<StringId, TopId>,
    ctx: &mut Context<'_>,
) {
    let name = ctx.interner.intern(top.name());

    if let Some(existing) = namespace.insert(name, top_id) {
        // TODO: #1343 Temporary solution until we implement scoping in the AST.
        if ctx.ast[existing]
            .name()
            .starts_with(ast::DYNAMIC_TYPE_NAME_PREFIX)
        {
            duplicate_names
                .dynamic_types
                .entry(name)
                .or_insert_with(indexmap::IndexSet::default)
                .insert(existing);
            duplicate_names
                .dynamic_types
                .entry(name)
                .or_insert_with(indexmap::IndexSet::default)
                .insert(top_id);
        } else {
            duplicate_names
                .tops
                .entry(name)
                .or_insert_with(indexmap::IndexSet::default)
                .insert(existing);
            duplicate_names
                .tops
                .entry(name)
                .or_insert_with(indexmap::IndexSet::default)
                .insert(top_id);
        }
    }
}

fn assert_is_not_a_reserved_scalar_type(ident: &ast::Identifier, ctx: &mut Context<'_>) {
    if StaticType::try_from_str(ident.name()).is_some() {
        ctx.push_error(DatamodelError::new_reserved_scalar_type_error(
            ident.name(),
            ident.span().clone(),
        ));
    }
}

fn check_for_duplicate_properties<'a>(
    top: &ast::Top,
    props: &'a [Field<Expression>],
    tmp_names: &mut HashSet<&'a str>,
    ctx: &mut Context<'_>,
) {
    tmp_names.clear();
    for arg in props {
        if !tmp_names.insert(arg.name()) {
            ctx.push_error(DatamodelError::new_duplicate_config_key_error(
                &format!("{} \"{}\"", top.get_type(), top.name()),
                arg.name(),
                arg.span().clone(),
            ));
        }
    }
}

fn validate_attribute_identifiers(with_attrs: &dyn WithAttributes, ctx: &mut Context<'_>) {
    for attribute in with_attrs.attributes() {
        validate_attribute_name(attribute, ctx.diagnostics);
    }
}
