//! CST node expansion for stream_* definitions.
//!
//! Clones original `CLASS_DEF/TYPE_ALIAS_DEF` `GreenNodes` and surgically transforms
//! them: replaces `TYPE_EXPR`, filters `@stream.*` attrs, adds `@sap.*` attrs.
//! Non-stream attributes (alias, description, skip, dynamic, etc.) pass through
//! automatically because the original `FIELD/CLASS_DEF` nodes are cloned.

use baml_compiler_syntax::{GreenNode, SyntaxKind, SyntaxNode};
use rowan::ast::AstNode as _;
use rustc_hash::FxHashMap;

use crate::{
    desugar::{
        PpirDesugaredClass, PpirDesugaredField, PpirDesugaredTypeAlias, PpirStreamStartsAs,
        extract_starts_as_text,
    },
    normalize::default_starts_as_semantic,
    ty::{PpirTy, PpirTypeAttrs},
};

//
// ──────────────────────────────────────── GREEN-LEVEL HELPERS ─────
//

/// Extract the field name from a FIELD `GreenNode` (first WORD child token).
fn extract_field_name(field_green: &rowan::GreenNodeData) -> Option<String> {
    for child in field_green.children() {
        if let rowan::NodeOrToken::Token(t) = child {
            let kind: SyntaxKind = t.kind().into();
            if kind == SyntaxKind::WORD {
                return Some(t.text().to_string());
            }
        }
    }
    None
}

/// Extract the full dotted name from a `BLOCK_ATTRIBUTE` `GreenNode`.
/// Joins WORD and `KW_DYNAMIC` children with ".".
fn extract_block_attr_name(ba_green: &rowan::GreenNodeData) -> Option<String> {
    let mut segments = Vec::new();
    let mut past_at = false;
    for child in ba_green.children() {
        if let rowan::NodeOrToken::Token(t) = child {
            let kind: SyntaxKind = t.kind().into();
            match kind {
                SyntaxKind::AT | SyntaxKind::AT_AT => {
                    past_at = true;
                }
                SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC if past_at => {
                    segments.push(t.text().to_string());
                }
                SyntaxKind::DOT => {} // skip dots
                _ => {}
            }
        }
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

/// Extract the full dotted name from an ATTRIBUTE `GreenNode`.
/// Joins WORD children (after the @) with ".".
fn extract_attr_name(attr_green: &rowan::GreenNodeData) -> Option<String> {
    let mut segments = Vec::new();
    let mut past_at = false;
    for child in attr_green.children() {
        if let rowan::NodeOrToken::Token(t) = child {
            let kind: SyntaxKind = t.kind().into();
            match kind {
                SyntaxKind::AT => {
                    past_at = true;
                }
                SyntaxKind::WORD if past_at => {
                    segments.push(t.text().to_string());
                }
                SyntaxKind::DOT => {} // skip dots
                _ => {}
            }
        }
        // Stop at ATTRIBUTE_ARGS node — don't descend into args
        if let rowan::NodeOrToken::Node(n) = child {
            let kind: SyntaxKind = n.kind().into();
            if kind == SyntaxKind::ATTRIBUTE_ARGS {
                break;
            }
        }
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

/// Copy leading whitespace/newline tokens from a `GreenNode` into the builder.
/// Stops at the first non-trivia child.
fn copy_leading_trivia(b: &mut Builder, green: &rowan::GreenNodeData) {
    for child in green.children() {
        if let rowan::NodeOrToken::Token(t) = child {
            let kind: SyntaxKind = t.kind().into();
            if matches!(kind, SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE) {
                b.token(kind, t.text());
                continue;
            }
        }
        break;
    }
}

/// Extract the class/alias name from a `CLASS_DEF` or `TYPE_ALIAS_DEF` `SyntaxNode`.
fn extract_def_name(node: &SyntaxNode) -> Option<String> {
    match node.kind() {
        SyntaxKind::CLASS_DEF => {
            if let Some(class_def) = baml_compiler_syntax::ast::ClassDef::cast(node.clone()) {
                return class_def.name().map(|t| t.text().to_string());
            }
            None
        }
        SyntaxKind::TYPE_ALIAS_DEF => {
            if let Some(alias_def) = baml_compiler_syntax::ast::TypeAliasDef::cast(node.clone()) {
                return alias_def.name().map(|t| t.text().to_string());
            }
            None
        }
        _ => None,
    }
}

//
// ──────────────────────────────────────── PUBLIC ENTRY POINT ─────
//

/// Build a `SOURCE_FILE` `GreenNode` containing all stream_* items for one origin file.
///
/// Clones original CST nodes and transforms them. Returns None if there are no
/// stream_* expansions.
pub fn build_stream_source_file(
    original_cst: &SyntaxNode,
    classes: &[PpirDesugaredClass],
    type_aliases: &[PpirDesugaredTypeAlias],
) -> Option<GreenNode> {
    if classes.is_empty() && type_aliases.is_empty() {
        return None;
    }

    // Index desugared classes and type aliases by name
    let class_map: FxHashMap<&str, &PpirDesugaredClass> =
        classes.iter().map(|c| (c.name.as_str(), c)).collect();
    let alias_map: FxHashMap<&str, &PpirDesugaredTypeAlias> =
        type_aliases.iter().map(|a| (a.name.as_str(), a)).collect();

    let mut b = Builder::new();
    b.start_node(SyntaxKind::SOURCE_FILE);

    let mut first = true;
    for child in original_cst.children() {
        match child.kind() {
            SyntaxKind::CLASS_DEF => {
                if let Some(name) = extract_def_name(&child) {
                    if let Some(class) = class_map.get(name.as_str()) {
                        if !first {
                            b.nl();
                        }
                        first = false;
                        build_stream_class_from_original(
                            &mut b,
                            &child.green().into_owned(),
                            class,
                        );
                    }
                }
            }
            SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(name) = extract_def_name(&child) {
                    if let Some(alias) = alias_map.get(name.as_str()) {
                        if !first {
                            b.nl();
                        }
                        first = false;
                        build_stream_type_alias_from_original(
                            &mut b,
                            &child.green().into_owned(),
                            alias,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    b.finish_node(); // SOURCE_FILE
    Some(b.finish())
}

//
// ──────────────────────────────────────── CLASS TRANSFORM ─────
//

/// Clone-and-transform an original `CLASS_DEF` into a stream_* `CLASS_DEF`.
fn build_stream_class_from_original(
    b: &mut Builder,
    original_green: &rowan::GreenNode,
    class: &PpirDesugaredClass,
) {
    // Index desugared fields by name for lookup
    let field_map: FxHashMap<&str, &PpirDesugaredField> =
        class.fields.iter().map(|f| (f.name.as_str(), f)).collect();

    b.start_node(SyntaxKind::CLASS_DEF);

    // Track whether we've seen @@stream.done (to emit @@sap.in_progress(never))
    let mut has_stream_done = false;
    // Track last WORD token index to identify class name
    let mut seen_kw_class = false;

    for child in original_green.children() {
        match child {
            rowan::NodeOrToken::Token(t) => {
                let kind: SyntaxKind = t.kind().into();
                match kind {
                    SyntaxKind::KW_CLASS => {
                        b.token(kind, t.text());
                        seen_kw_class = true;
                    }
                    SyntaxKind::WORD if seen_kw_class => {
                        // This is the class name — emit stream_ prefix
                        b.token(SyntaxKind::WORD, &format!("stream_{}", t.text()));
                        seen_kw_class = false; // only first WORD after KW_CLASS
                    }
                    _ => {
                        // Copy as-is (whitespace, braces, etc.)
                        b.token(kind, t.text());
                    }
                }
            }
            rowan::NodeOrToken::Node(n) => {
                let kind: SyntaxKind = n.kind().into();
                match kind {
                    SyntaxKind::FIELD => {
                        if let Some(field_name) = extract_field_name(n) {
                            if let Some(ef) = field_map.get(field_name.as_str()) {
                                // Compute S | D and check if field should be emitted
                                let s = compute_s_type(ef);
                                let d = ef.stream_type.clone();
                                if matches!((&s, &d), (PpirTy::Never { .. }, PpirTy::Never { .. }))
                                {
                                    // Omit field — both S and D are never
                                    continue;
                                }
                                let stream_type = PpirTy::Union {
                                    variants: vec![s, d],
                                    attrs: PpirTypeAttrs::default(),
                                };
                                transform_field(b, n, ef, &stream_type);
                            }
                            // If not found in field_map, the field was omitted
                            // (both S and D were never) — skip it
                        } else {
                            // Can't extract name — copy as-is (defensive)
                            b.copy_green_node(n);
                        }
                    }
                    SyntaxKind::BLOCK_ATTRIBUTE => {
                        if let Some(attr_name) = extract_block_attr_name(n) {
                            match attr_name.as_str() {
                                "stream.done" => {
                                    has_stream_done = true;
                                    // Copy leading whitespace from the original BLOCK_ATTRIBUTE
                                    copy_leading_trivia(b, n);
                                    // Replace with @@sap.in_progress(never)
                                    build_block_attribute(b, "sap.in_progress", Some("never"));
                                }
                                "stream.not_null" => {
                                    // Remove — already distributed to fields
                                }
                                name if name.starts_with("stream.") => {
                                    // Remove other @@stream.* attrs
                                }
                                _ => {
                                    // Copy as-is (@@dynamic, etc.)
                                    b.copy_green_node(n);
                                }
                            }
                        } else {
                            b.copy_green_node(n);
                        }
                    }
                    _ => {
                        // Copy other nodes as-is
                        b.copy_green_node(n);
                    }
                }
            }
        }
    }

    // If @@stream.done was NOT found as a block attribute but class_stream_done
    // might have been set via other means, we already handle it above.
    // The `has_stream_done` variable is set when we encounter it.
    let _ = has_stream_done; // used above in the replacement

    b.finish_node(); // CLASS_DEF
}

//
// ──────────────────────────────────────── FIELD TRANSFORM ─────
//

/// Clone-and-transform an original FIELD node into a stream_* field.
///
/// - `TYPE_EXPR` is replaced with the expanded stream type + @`sap.in_progress` if needed
/// - @stream.* ATTRIBUTE children are removed
/// - Other ATTRIBUTEs (@alias, @description, @skip, etc.) are copied as-is
/// - SAP field attributes are appended after all original content
fn transform_field(
    b: &mut Builder,
    original_field_green: &rowan::GreenNodeData,
    ef: &PpirDesugaredField,
    stream_type: &PpirTy,
) {
    b.start_node(SyntaxKind::FIELD);

    for child in original_field_green.children() {
        match child {
            rowan::NodeOrToken::Token(t) => {
                let kind: SyntaxKind = t.kind().into();
                // Copy field name (WORD) and all other tokens (whitespace, etc.) as-is
                b.token(kind, t.text());
            }
            rowan::NodeOrToken::Node(n) => {
                let kind: SyntaxKind = n.kind().into();
                match kind {
                    SyntaxKind::TYPE_EXPR => {
                        // The original TYPE_EXPR may contain leading whitespace
                        // that separates the field name from the type. Since we're
                        // replacing the TYPE_EXPR, we need to ensure whitespace
                        // is present before the new TYPE_EXPR.
                        b.ws(" ");
                        // Replace with new TYPE_EXPR
                        b.start_node(SyntaxKind::TYPE_EXPR);
                        build_type_expr_inner(b, stream_type);
                        if ef.sap_in_progress_never {
                            b.ws(" ");
                            build_attribute(b, "sap.in_progress", Some("never"));
                        }
                        b.finish_node(); // TYPE_EXPR
                    }
                    SyntaxKind::ATTRIBUTE => {
                        // Check if this is a @stream.* attribute — if so, remove it
                        if let Some(attr_name) = extract_attr_name(n) {
                            if attr_name.starts_with("stream.") {
                                // Remove @stream.* attributes (safety net; normally inside TYPE_EXPR)
                                continue;
                            }
                        }
                        // Copy non-stream attributes as-is (@alias, @description, @skip, etc.)
                        b.copy_green_node(n);
                    }
                    _ => {
                        b.copy_green_node(n);
                    }
                }
            }
        }
    }

    // Append SAP field attributes
    let sap_starts_as_text = compute_sap_starts_as_text(ef);
    b.ws(" ");
    build_attribute(
        b,
        "sap.class_completed_field_missing",
        Some(&sap_starts_as_text),
    );
    b.ws(" ");
    build_attribute(
        b,
        "sap.class_in_progress_field_missing",
        Some(&sap_starts_as_text),
    );

    b.finish_node(); // FIELD
}

//
// ──────────────────────────────────────── TYPE ALIAS TRANSFORM ─────
//

/// Clone-and-transform an original `TYPE_ALIAS_DEF` into a stream_* type alias.
fn build_stream_type_alias_from_original(
    b: &mut Builder,
    original_green: &rowan::GreenNode,
    alias: &PpirDesugaredTypeAlias,
) {
    b.start_node(SyntaxKind::TYPE_ALIAS_DEF);

    // In TYPE_ALIAS_DEF: first WORD is "type", second WORD is the name
    let mut word_count = 0;

    for child in original_green.children() {
        match child {
            rowan::NodeOrToken::Token(t) => {
                let kind: SyntaxKind = t.kind().into();
                if kind == SyntaxKind::WORD {
                    word_count += 1;
                    if word_count == 1 {
                        // "type" keyword — copy as-is
                        b.token(kind, t.text());
                    } else if word_count == 2 {
                        // Alias name — emit stream_ prefix
                        b.token(SyntaxKind::WORD, &format!("stream_{}", t.text()));
                    } else {
                        b.token(kind, t.text());
                    }
                } else {
                    b.token(kind, t.text());
                }
            }
            rowan::NodeOrToken::Node(n) => {
                let kind: SyntaxKind = n.kind().into();
                if kind == SyntaxKind::TYPE_EXPR {
                    // Replace with expanded body
                    b.start_node(SyntaxKind::TYPE_EXPR);
                    build_type_expr_inner(b, &alias.expanded_body);
                    b.finish_node(); // TYPE_EXPR
                } else {
                    b.copy_green_node(n);
                }
            }
        }
    }

    b.finish_node(); // TYPE_ALIAS_DEF
}

//
// ──────────────────────────────────────── SHARED HELPERS ─────
//

/// Compute the S (starts-as) type for a field's union S | D.
fn compute_s_type(ef: &PpirDesugaredField) -> PpirTy {
    match &ef.sap_starts_as {
        PpirStreamStartsAs::Never => PpirTy::Never {
            attrs: PpirTypeAttrs::default(),
        },
        PpirStreamStartsAs::DefaultFor(ty) => ty.clone(),
        PpirStreamStartsAs::Explicit { typeof_s, .. } => typeof_s.clone(),
    }
}

/// Compute the @sap.class_*_`field_missing` attribute argument text for a field.
fn compute_sap_starts_as_text(ef: &PpirDesugaredField) -> String {
    match &ef.sap_starts_as {
        PpirStreamStartsAs::Never => "never".to_string(),
        PpirStreamStartsAs::DefaultFor(ty) => {
            let starts_as = default_starts_as_semantic(ty);
            format!("{starts_as}")
        }
        PpirStreamStartsAs::Explicit { green, .. } => extract_starts_as_text(green),
    }
}

/// Build the inner content of a `TYPE_EXPR` node from a `PpirTy`.
///
/// This is called within an already-opened `TYPE_EXPR` node. It emits
/// the tokens/child nodes that represent the type.
fn build_type_expr_inner(b: &mut Builder, ty: &PpirTy) {
    match ty {
        PpirTy::Named { name, .. } => {
            b.token(SyntaxKind::WORD, name.as_str());
        }
        PpirTy::Int { .. } => b.token(SyntaxKind::WORD, "int"),
        PpirTy::Float { .. } => b.token(SyntaxKind::WORD, "float"),
        PpirTy::String { .. } => b.token(SyntaxKind::WORD, "string"),
        PpirTy::Bool { .. } => b.token(SyntaxKind::WORD, "bool"),
        PpirTy::Null { .. } => b.token(SyntaxKind::WORD, "null"),
        PpirTy::Never { .. } => b.token(SyntaxKind::WORD, "never"),

        PpirTy::Optional { inner, .. } => {
            build_type_expr_inner(b, inner);
            b.token(SyntaxKind::QUESTION, "?");
        }

        PpirTy::List { inner, .. } => {
            build_type_expr_inner(b, inner);
            b.token(SyntaxKind::L_BRACKET, "[");
            b.token(SyntaxKind::R_BRACKET, "]");
        }

        PpirTy::Map { key, value, .. } => {
            b.token(SyntaxKind::WORD, "map");
            b.token(SyntaxKind::LESS, "<");
            build_type_expr_inner(b, key);
            b.token(SyntaxKind::COMMA, ",");
            b.ws(" ");
            build_type_expr_inner(b, value);
            b.token(SyntaxKind::GREATER, ">");
        }

        PpirTy::Union { variants, .. } => {
            // Wrap in parens to be safe for nested unions
            b.token(SyntaxKind::L_PAREN, "(");
            for (i, v) in variants.iter().enumerate() {
                if i > 0 {
                    b.ws(" ");
                    b.token(SyntaxKind::PIPE, "|");
                    b.ws(" ");
                }
                build_type_expr_inner(b, v);
            }
            b.token(SyntaxKind::R_PAREN, ")");
        }

        PpirTy::StringLiteral { value, .. } => {
            // Emit as a quoted string literal in the type position
            b.token(SyntaxKind::QUOTE, "\"");
            b.token(SyntaxKind::WORD, value);
            b.token(SyntaxKind::QUOTE, "\"");
        }

        PpirTy::IntLiteral { value, .. } => {
            b.token(SyntaxKind::WORD, &value.to_string());
        }

        PpirTy::BoolLiteral { value, .. } => {
            b.token(SyntaxKind::WORD, if *value { "true" } else { "false" });
        }

        PpirTy::Media { kind, .. } => {
            let name = match kind {
                baml_base::MediaKind::Image => "image",
                baml_base::MediaKind::Audio => "audio",
                baml_base::MediaKind::Video => "video",
                baml_base::MediaKind::Pdf => "pdf",
                baml_base::MediaKind::Generic => "image",
            };
            b.token(SyntaxKind::WORD, name);
        }

        PpirTy::Unknown { .. } => {
            b.token(SyntaxKind::WORD, "unknown");
        }
    }
}

//
// ──────────────────────────────────────── ATTRIBUTE BUILDING ─────
//

/// Build an @name or @name.sub attribute node with optional argument.
fn build_attribute(b: &mut Builder, name: &str, arg: Option<&str>) {
    b.start_node(SyntaxKind::ATTRIBUTE);
    b.token(SyntaxKind::AT, "@");

    // Split dotted name into segments
    let segments: Vec<&str> = name.split('.').collect();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            b.token(SyntaxKind::DOT, ".");
        }
        b.token(SyntaxKind::WORD, seg);
    }

    if let Some(arg_text) = arg {
        emit_attribute_args(b, arg_text);
    }

    b.finish_node(); // ATTRIBUTE
}

/// Build a @@name block attribute with optional argument.
fn build_block_attribute(b: &mut Builder, name: &str, arg: Option<&str>) {
    b.start_node(SyntaxKind::BLOCK_ATTRIBUTE);
    b.token(SyntaxKind::AT, "@");
    b.token(SyntaxKind::AT, "@");

    let segments: Vec<&str> = name.split('.').collect();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            b.token(SyntaxKind::DOT, ".");
        }
        b.token(SyntaxKind::WORD, seg);
    }

    if let Some(arg_text) = arg {
        emit_attribute_args(b, arg_text);
    }

    b.finish_node(); // BLOCK_ATTRIBUTE
}

/// Emit `ATTRIBUTE_ARGS` node. Quotes the value if it contains non-word characters.
fn emit_attribute_args(b: &mut Builder, arg_text: &str) {
    b.start_node(SyntaxKind::ATTRIBUTE_ARGS);
    b.token(SyntaxKind::L_PAREN, "(");
    if arg_text.chars().all(|c| c.is_alphanumeric() || c == '_') {
        b.token(SyntaxKind::WORD, arg_text);
    } else {
        // Quote values that contain non-word chars (e.g. "Loading...")
        b.token(SyntaxKind::QUOTE, "\"");
        b.token(SyntaxKind::WORD, arg_text);
        b.token(SyntaxKind::QUOTE, "\"");
    }
    b.token(SyntaxKind::R_PAREN, ")");
    b.finish_node(); // ATTRIBUTE_ARGS
}

//
// ──────────────────────────────────────── BUILDER WRAPPER ─────
//

/// Thin wrapper around `GreenNodeBuilder` for building CST trees.
struct Builder {
    inner: rowan::GreenNodeBuilder<'static>,
}

impl Builder {
    fn new() -> Self {
        Self {
            inner: rowan::GreenNodeBuilder::new(),
        }
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.inner.start_node(kind.into());
    }

    fn finish_node(&mut self) {
        self.inner.finish_node();
    }

    fn token(&mut self, kind: SyntaxKind, text: &str) {
        self.inner.token(kind.into(), text);
    }

    fn ws(&mut self, text: &str) {
        self.token(SyntaxKind::WHITESPACE, text);
    }

    fn nl(&mut self) {
        self.token(SyntaxKind::NEWLINE, "\n");
    }

    fn finish(self) -> GreenNode {
        self.inner.finish()
    }

    /// Recursively copy an existing `GreenNode` subtree into the builder.
    fn copy_green_node(&mut self, green: &rowan::GreenNodeData) {
        let kind: SyntaxKind = green.kind().into();
        self.start_node(kind);
        for child in green.children() {
            match child {
                rowan::NodeOrToken::Node(n) => self.copy_green_node(n),
                rowan::NodeOrToken::Token(t) => self.copy_green_token(t),
            }
        }
        self.finish_node();
    }

    /// Copy a single green token into the builder.
    fn copy_green_token(&mut self, token: &rowan::GreenTokenData) {
        let kind: SyntaxKind = token.kind().into();
        self.token(kind, token.text());
    }
}
