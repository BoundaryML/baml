//! Item declarations and attributes: what opens a declaration where the
//! cursor stands, and which `@attribute` names the compiler accepts.
//!
//! Both lists are the compiler's own: the keyword sets mirror the item
//! forms the grammar parses in each container, and the attribute names are
//! the SAME constants validation reads
//! ([`FIELD_ATTR_NAMES`](baml_compiler2_ast::FIELD_ATTR_NAMES),
//! [`KNOWN_TYPE_ATTRS`](baml_compiler2_hir::KNOWN_TYPE_ATTRS)) —
//! what is offered is what checks.

use super::{completions::Completions, context::ItemContainer};

/// Declarations the grammar accepts at the top of a file, each with the
/// snippet that writes its skeleton (tab stops per the LSP snippet grammar;
/// the accumulator's plain-text downgrade collapses them for clients
/// without snippet support).
const TOP_LEVEL: &[(&str, &str)] = &[
    ("class", "class ${1:Name} {\n\t$0\n}"),
    ("enum", "enum ${1:Name} {\n\t$0\n}"),
    ("interface", "interface ${1:Name} {\n\t$0\n}"),
    ("function", "function ${1:name}($2) -> $3 {\n\t$0\n}"),
    (
        "implement",
        "implement ${1:Interface} for ${2:Type} {\n\t$0\n}",
    ),
    ("type", "type ${1:Name} = $0"),
    ("let", "let ${1:name} = $0;"),
    ("client", "client ${1:Name} = $0"),
    ("generator", "generator ${1:name} {\n\t$0\n}"),
    ("test", "test \"${1:name}\" {\n\t$0\n}"),
    ("testset", "testset \"${1:name}\" {\n\t$0\n}"),
    ("retry_policy", "retry_policy ${1:name} {\n\t$0\n}"),
    ("template_string", "template_string ${1:name}($2) #\"$0\"#"),
];

/// Declarations the grammar accepts inside a `class` body (fields are the
/// reader's own names; nothing to offer for those). The `self` receiver is
/// a deletable placeholder: statics simply remove it.
const CLASS_BODY: &[(&str, &str)] = &[
    ("function", "function ${1:name}(${2:self}) -> $3 {\n\t$0\n}"),
    ("implements", "implements ${1:Interface} {\n\t$0\n}"),
];

/// Declarations the grammar accepts inside an `interface` body. The method
/// skeleton is the REQUIRED (bodyless) form, and it spells the `throws`
/// clause because interface signatures must declare one (E0170) — the
/// snippet teaches the rule instead of leaving the reader to hit it.
const INTERFACE_BODY: &[(&str, &str)] = &[
    (
        "function",
        "function ${1:name}(${2:self}) -> $3 throws ${4:never}",
    ),
    ("type", "type ${1:Name}"),
];

/// Declarations the grammar accepts inside an `implement … for` body.
const IMPL_BODY: &[(&str, &str)] = &[
    ("function", "function ${1:name}(${2:self}) -> $3 {\n\t$0\n}"),
    ("type", "type ${1:Name} = $0"),
];

pub(crate) fn complete_items(container: ItemContainer, out: &mut Completions) {
    let declarations = match container {
        ItemContainer::TopLevel => TOP_LEVEL,
        ItemContainer::Class => CLASS_BODY,
        ItemContainer::Interface => INTERFACE_BODY,
        ItemContainer::Impl => IMPL_BODY,
    };
    for (keyword, snippet) in declarations {
        out.add_declaration(keyword, snippet);
    }
}

pub(crate) fn complete_attributes(out: &mut Completions) {
    // Unknown `@` names on FIELDS are legal user schema annotations (hoisted
    // for reflection read-back), so this list is a menu of the names the
    // compiler gives meaning to, not a closed set.
    for name in baml_compiler2_ast::FIELD_ATTR_NAMES {
        out.add_attribute(name);
    }
    for name in baml_compiler2_hir::KNOWN_TYPE_ATTRS {
        out.add_attribute(name);
    }
}
