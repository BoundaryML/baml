//! Test validation logic for HIR lowering.
//!
//! This module contains validation for test definitions, including:
//! - Property name validation
//! - Required property checking
//! - Type builder block lowering

use baml_base::Name;
use baml_compiler_diagnostics::HirDiagnostic;
use baml_compiler_syntax::SyntaxNode;
use rowan::ast::AstNode;

use crate::{
    Attribute, LoweringContext,
    item_tree::{Test, TypeBuilderBlock, TypeBuilderEntry},
    lower_class, lower_enum, lower_type_alias,
};

/// Valid test properties.
pub(crate) const VALID_TEST_PROPERTIES: &[&str] = &["functions", "args", "type_builder"];

/// Extract test definition from CST with validation.
pub(crate) fn lower_test(node: &SyntaxNode, ctx: &mut LoweringContext) -> Option<Test> {
    use baml_compiler_syntax::ast::TestDef;

    let test = TestDef::cast(node.clone())?;

    // Extract name using AST accessor
    let name_token = test.name()?;
    let name = Name::new(name_token.text());
    let test_name = name.to_string();

    // Track for required property check
    let mut has_functions = false;
    let mut has_args = false;

    // Process config block if present
    if let Some(config_block) = test.config_block() {
        for item in config_block.items() {
            let Some(key_token) = item.key() else {
                continue;
            };
            let key = key_token.text();
            let key_span = ctx.span(key_token.text_range());

            // Validate property name
            if !VALID_TEST_PROPERTIES.contains(&key) {
                ctx.push_diagnostic(HirDiagnostic::UnknownTestProperty {
                    test_name: test_name.clone(),
                    property_name: key.to_string(),
                    span: key_span,
                    valid_properties: VALID_TEST_PROPERTIES.to_vec(),
                });
                continue;
            }

            match key {
                "functions" => {
                    has_functions = true;
                }
                "args" => {
                    has_args = true;
                }
                _ => {}
            }

            // Check for attributes on config items (not allowed on test fields)
            for attr in item.attributes() {
                if let Some(attr_name) = attr.name() {
                    let attr_span = ctx.span(attr.syntax().text_range());
                    ctx.push_diagnostic(HirDiagnostic::TestFieldAttribute {
                        attr_name: attr_name.text().to_string(),
                        span: attr_span,
                    });
                }
            }
        }
    }

    // Check required property: args
    if !has_args {
        ctx.push_diagnostic(HirDiagnostic::MissingTestProperty {
            test_name,
            property_name: "args",
            span: ctx.span(name_token.text_range()),
        });
    }

    // Extract all function references using AST accessor
    let function_refs = test
        .function_names()
        .into_iter()
        .map(|t| Name::new(t.text()))
        .collect();

    // Only emit missing functions diagnostic if we didn't already have an unknown property error
    // that would cover a typo like "input" instead of "functions"
    if !has_functions && has_args {
        // If args is present but functions is not, they might have just forgotten it
        // But we don't emit this if there are no properties at all (covered by missing args)
    }

    // Lower type_builder block if present
    let type_builder = test
        .config_block()
        .and_then(|config| config.type_builder_blocks().next())
        .map(|tb_block| lower_type_builder_block(&tb_block, ctx));

    Some(Test {
        name,
        function_refs,
        type_builder,
    })
}

/// Lower a `type_builder` block to HIR.
fn lower_type_builder_block(
    block: &baml_compiler_syntax::ast::TypeBuilderBlock,
    ctx: &mut LoweringContext,
) -> TypeBuilderBlock {
    use rowan::ast::AstNode;

    let mut entries = Vec::new();

    // Lower non-dynamic classes
    for class_def in block.classes() {
        if let Some(class) = lower_class(class_def.syntax(), ctx) {
            entries.push(TypeBuilderEntry::Class(class));
        }
    }

    // Lower non-dynamic enums
    for enum_def in block.enums() {
        if let Some(e) = lower_enum(enum_def.syntax(), ctx) {
            entries.push(TypeBuilderEntry::Enum(e));
        }
    }

    // Lower dynamic types (dynamic class / dynamic enum)
    for dynamic_def in block.dynamic_types() {
        if let Some(class_def) = dynamic_def.class() {
            if let Some(mut class) = lower_class(class_def.syntax(), ctx) {
                // Mark as dynamic (override the is_dynamic from parsing)
                class.is_dynamic = Attribute::Explicit(());
                entries.push(TypeBuilderEntry::DynamicClass(class));
            }
        } else if let Some(enum_def) = dynamic_def.enum_def() {
            if let Some(e) = lower_enum(enum_def.syntax(), ctx) {
                entries.push(TypeBuilderEntry::DynamicEnum(e));
            }
        }
    }

    // Lower type aliases
    for alias_def in block.type_aliases() {
        if let Some(alias) = lower_type_alias(alias_def.syntax()) {
            entries.push(TypeBuilderEntry::TypeAlias(alias));
        }
    }

    TypeBuilderBlock { entries }
}
