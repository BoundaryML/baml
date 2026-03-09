//! Extract `$rust_function` builtins from the compiler2 `.baml` stdlib files.
//!
//! Iterates `baml_builtins2::ALL` for the `"baml"` package, parses each file
//! through the compiler2 front-end (lex → parse → lower), and collects every
//! function whose body is `FunctionBodyDef::Builtin(BuiltinKind::Vm)` into a
//! `NativeBuiltin` record. The CST is also retained per file for
//! `//baml:mut_self` directive scanning.

use baml_base::FileId;
use baml_compiler2_ast::ast::{BuiltinKind, ClassDef, FunctionBodyDef, FunctionDef, Item, TypeExpr};
use baml_compiler_syntax::{NodeOrToken, SyntaxKind, SyntaxNode};

use crate::types::{BamlType, NativeBuiltin, Param, Receiver};

/// Parse, lower, and extract all `$rust_function` builtins from the `.baml` stdlib.
///
/// Only processes files with `package == "baml"`. Skips `$rust_io_function`
/// builtins entirely (those stay on the legacy pipeline).
pub fn extract_native_builtins() -> Vec<NativeBuiltin> {
    let mut builtins = Vec::new();

    for builtin_file in baml_builtins2::ALL {
        if builtin_file.package != "baml" {
            continue;
        }

        // Lex and parse into a lossless CST.
        let tokens = baml_compiler_lexer::lex_lossless(builtin_file.contents, FileId::new(0));
        let (green, errors) = baml_compiler_parser::parse_file(&tokens);
        if !errors.is_empty() {
            // Skip files that fail to parse (shouldn't happen in practice).
            continue;
        }
        let cst_root = SyntaxNode::new_root(green);

        // Lower CST → AST items.
        let (items, _diags) = baml_compiler2_ast::lower_file(&cst_root);

        // Build the namespace prefix from the file's namespace slices.
        // e.g. namespace = &["math"] → namespace_prefix = "baml.math"
        //      namespace = &[]       → namespace_prefix = "baml"
        let namespace_prefix = if builtin_file.namespace.is_empty() {
            "baml".to_string()
        } else {
            format!("baml.{}", builtin_file.namespace.join("."))
        };

        for item in &items {
            match item {
                Item::Class(class_def) => {
                    extract_from_class(class_def, &namespace_prefix, &cst_root, &mut builtins);
                }
                Item::Function(func_def) => {
                    extract_from_free_function(func_def, &namespace_prefix, &mut builtins);
                }
                _ => {}
            }
        }
    }

    builtins
}

/// Extract `$rust_function` methods from a class definition.
fn extract_from_class(
    class_def: &ClassDef,
    namespace_prefix: &str,
    cst_root: &SyntaxNode,
    builtins: &mut Vec<NativeBuiltin>,
) {
    let class_name = class_def.name.as_str();
    let class_generics: Vec<String> = class_def
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();

    for method in &class_def.methods {
        if !is_vm_builtin(method) {
            continue;
        }

        // Merge class generics with method-level generics (method generics first isn't
        // common, but handle it).
        let method_generics: Vec<String> = method
            .generic_params
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        let mut all_generics = class_generics.clone();
        for g in &method_generics {
            if !all_generics.contains(g) {
                all_generics.push(g.clone());
            }
        }

        // Build the dotted path: "baml.Array.length" or "baml.media.Pdf.url"
        let path = format!("{namespace_prefix}.{class_name}.{}", method.name.as_str());

        // Derive fn_name: dots→underscores, lowercase.
        // e.g. "baml.Array.length" → "baml_array_length"
        // Note: class name stays as-is in the fn_name (lowercased).
        let fn_name = path_to_fn_name(&path);

        // Detect whether this is an instance method (has `self` first param) or a
        // static/constructor method (no `self`). Static methods like `Pdf.from_url(url, mime_type)`
        // are defined inside the class but don't take `self`.
        let has_self = method
            .params
            .first()
            .map(|p| p.name.as_str() == "self")
            .unwrap_or(false);

        let (params, receiver) = if has_self {
            // Instance method: skip `self`, create receiver.
            let params = extract_params_skip_self(method, &all_generics);
            let is_mut = has_mut_self_directive(cst_root, class_name, method.name.as_str());
            let receiver = Some(Receiver {
                class_name: class_name.to_string(),
                class_generics: class_generics.clone(),
                is_mut,
            });
            (params, receiver)
        } else {
            // Static/constructor method: all params are regular params, no receiver.
            let params: Vec<Param> = method
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.as_str().to_string(),
                    ty: p
                        .type_expr
                        .as_ref()
                        .map(|te| type_expr_to_baml_type(&te.expr, &all_generics))
                        .unwrap_or(BamlType::Named("unknown".to_string())),
                })
                .collect();
            (params, None)
        };

        // Determine return type.
        let return_type = method
            .return_type
            .as_ref()
            .map(|te| type_expr_to_baml_type(&te.expr, &all_generics))
            .unwrap_or(BamlType::Null);

        builtins.push(NativeBuiltin {
            path,
            fn_name,
            params,
            return_type,
            generics: all_generics,
            receiver,
        });
    }
}

/// Extract a `$rust_function` free function (not inside a class).
fn extract_from_free_function(
    func_def: &FunctionDef,
    namespace_prefix: &str,
    builtins: &mut Vec<NativeBuiltin>,
) {
    if !is_vm_builtin(func_def) {
        return;
    }

    let generics: Vec<String> = func_def
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();

    let path = format!("{namespace_prefix}.{}", func_def.name.as_str());
    let fn_name = path_to_fn_name(&path);

    // Free functions have no `self` — all params are regular params.
    let params: Vec<Param> = func_def
        .params
        .iter()
        .map(|p| Param {
            name: p.name.as_str().to_string(),
            ty: p
                .type_expr
                .as_ref()
                .map(|te| type_expr_to_baml_type(&te.expr, &generics))
                .unwrap_or(BamlType::Named("unknown".to_string())),
        })
        .collect();

    let return_type = func_def
        .return_type
        .as_ref()
        .map(|te| type_expr_to_baml_type(&te.expr, &generics))
        .unwrap_or(BamlType::Null);

    builtins.push(NativeBuiltin {
        path,
        fn_name,
        params,
        return_type,
        generics,
        receiver: None,
    });
}

/// Returns true if the function body is `$rust_function` (VM builtin).
fn is_vm_builtin(func: &FunctionDef) -> bool {
    matches!(func.body, Some(FunctionBodyDef::Builtin(BuiltinKind::Vm)))
}

/// Convert a dotted path to a Rust function name.
///
/// Examples:
/// - `"baml.Array.length"` → `"baml_array_length"`
/// - `"baml.deep_copy"` → `"baml_deep_copy"`
/// - `"baml.math.trunc"` → `"baml_math_trunc"`
/// - `"baml.media.Pdf.url"` → `"baml_media_pdf_url"`
fn path_to_fn_name(path: &str) -> String {
    path.replace('.', "_").to_lowercase()
}

/// Extract parameters from a method, skipping the first `self` parameter.
fn extract_params_skip_self(func: &FunctionDef, generics: &[String]) -> Vec<Param> {
    func.params
        .iter()
        .skip(1) // skip `self`
        .map(|p| Param {
            name: p.name.as_str().to_string(),
            ty: p
                .type_expr
                .as_ref()
                .map(|te| type_expr_to_baml_type(&te.expr, generics))
                .unwrap_or(BamlType::Named("unknown".to_string())),
        })
        .collect()
}

/// Convert a `TypeExpr` from the AST to a `BamlType`.
///
/// `generics` is the combined set of type parameter names in scope (class + method).
fn type_expr_to_baml_type(ty: &TypeExpr, generics: &[String]) -> BamlType {
    match ty {
        TypeExpr::Int => BamlType::Int,
        TypeExpr::Float => BamlType::Float,
        TypeExpr::String => BamlType::String,
        TypeExpr::Bool => BamlType::Bool,
        TypeExpr::Null => BamlType::Null,
        TypeExpr::Never => BamlType::Null,

        TypeExpr::Media(kind) => {
            // Map MediaKind to the class name string.
            let name = match kind {
                baml_base::MediaKind::Image => "Image",
                baml_base::MediaKind::Audio => "Audio",
                baml_base::MediaKind::Video => "Video",
                baml_base::MediaKind::Pdf => "Pdf",
                baml_base::MediaKind::Generic => "Media",
            };
            BamlType::Media(name.to_string())
        }

        TypeExpr::Optional(inner) => {
            BamlType::Optional(Box::new(type_expr_to_baml_type(inner, generics)))
        }

        TypeExpr::List(inner) => {
            BamlType::List(Box::new(type_expr_to_baml_type(inner, generics)))
        }

        TypeExpr::Map { key, value } => BamlType::Map(
            Box::new(type_expr_to_baml_type(key, generics)),
            Box::new(type_expr_to_baml_type(value, generics)),
        ),

        TypeExpr::Path(segments) => {
            // Single-segment path may be a generic type param or a named type.
            if segments.len() == 1 {
                let name = segments[0].as_str();
                if generics.iter().any(|g| g == name) {
                    BamlType::Generic(name.to_string())
                } else {
                    BamlType::Named(name.to_string())
                }
            } else {
                // Multi-segment path (e.g. `baml.errors.Io`) — treat as Named.
                let name = segments
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                BamlType::Named(name)
            }
        }

        // Treat everything else (Union, Literal, Function, BuiltinUnknown, etc.) as Named.
        TypeExpr::Union(_) => BamlType::Named("union".to_string()),
        TypeExpr::Literal(_) => BamlType::Named("literal".to_string()),
        TypeExpr::Function { .. } => BamlType::Named("function".to_string()),
        TypeExpr::BuiltinUnknown | TypeExpr::Unknown | TypeExpr::Error => {
            BamlType::Named("unknown".to_string())
        }
        TypeExpr::Type => BamlType::Named("type".to_string()),
        TypeExpr::Rust => BamlType::Named("rust".to_string()),
    }
}

/// Check if the function named `method_name` inside the class named `class_name`
/// has a `//baml:mut_self` comment inside the function node before the `function` keyword.
///
/// In the Rowan CST, the parser's `bump()` emits leading trivia tokens (whitespace,
/// comments) immediately before the `function` keyword inside the `FUNCTION_DEF` node
/// itself. So `//baml:mut_self` appears as a `LINE_COMMENT` token child of the
/// `FUNCTION_DEF` node, before the `KW_FUNCTION` token.
fn has_mut_self_directive(cst_root: &SyntaxNode, class_name: &str, method_name: &str) -> bool {
    for class_node in cst_root.descendants() {
        if class_node.kind() != SyntaxKind::CLASS_DEF {
            continue;
        }

        // Check if this class has the right name.
        if !class_node_has_name(&class_node, class_name) {
            continue;
        }

        // Find all FUNCTION_DEF descendants of this class node.
        for func_node in class_node.descendants() {
            if func_node.kind() != SyntaxKind::FUNCTION_DEF {
                continue;
            }

            // Check the function name.
            if !func_node_has_name(&func_node, method_name) {
                continue;
            }

            // Found the function. The `//baml:mut_self` comment is emitted as a
            // LINE_COMMENT token inside the FUNCTION_DEF node (as leading trivia
            // before the `function` keyword). Scan tokens inside the node that
            // appear before the KW_FUNCTION token.
            if function_node_has_mut_self_leading_comment(&func_node) {
                return true;
            }
        }
    }

    false
}

/// Returns true if the CLASS_DEF node has a name token matching `class_name`.
fn class_node_has_name(class_node: &SyntaxNode, class_name: &str) -> bool {
    // The class name is the first WORD token that is a direct meaningful child.
    // In the CST: `class WORD<...> { ... }`
    // Scan children_with_tokens: skip the `class` keyword and trivia,
    // then the next WORD should be the class name.
    for element in class_node.children_with_tokens() {
        if let NodeOrToken::Token(tok) = element {
            if tok.kind().is_trivia() || tok.kind() == SyntaxKind::KW_CLASS {
                continue;
            }
            // First non-trivia, non-keyword token should be the class name.
            return tok.kind() == SyntaxKind::WORD && tok.text() == class_name;
        }
        // Encountered a child node before finding the name token — not a match.
        // (Shouldn't happen for CLASS_DEF in practice.)
    }
    false
}

/// Returns true if the FUNCTION_DEF node has a name matching `method_name`.
fn func_node_has_name(func_node: &SyntaxNode, method_name: &str) -> bool {
    for element in func_node.children_with_tokens() {
        if let NodeOrToken::Token(tok) = element {
            if tok.kind().is_trivia() || tok.kind() == SyntaxKind::KW_FUNCTION {
                continue;
            }
            // First non-trivia, non-keyword token should be the function name.
            return tok.kind() == SyntaxKind::WORD && tok.text() == method_name;
        }
        // Encountered a child node — past the name.
        break;
    }
    false
}

/// Check whether a `FUNCTION_DEF` node contains a `//baml:mut_self` `LINE_COMMENT`
/// token before its `KW_FUNCTION` token.
///
/// The parser emits trivia (whitespace, comments) as tokens within the containing
/// syntactic node before the first real token. So any `//baml:mut_self` that
/// appears immediately before `function push(...)` in source is stored as a
/// `LINE_COMMENT` child of that `FUNCTION_DEF` node.
fn function_node_has_mut_self_leading_comment(func_node: &SyntaxNode) -> bool {
    for element in func_node.children_with_tokens() {
        match element {
            NodeOrToken::Token(tok) => {
                match tok.kind() {
                    SyntaxKind::LINE_COMMENT => {
                        let text = tok.text().trim();
                        if text == "//baml:mut_self" {
                            return true;
                        }
                        // A different comment — keep scanning.
                    }
                    k if k.is_whitespace() => {
                        // Skip whitespace/newlines.
                    }
                    SyntaxKind::KW_FUNCTION => {
                        // Reached the `function` keyword — stop, no directive found.
                        return false;
                    }
                    SyntaxKind::AT_AT | SyntaxKind::BLOCK_COMMENT => {
                        // Block attributes or block comments — keep scanning.
                    }
                    _ => {
                        // Any other token — stop.
                        return false;
                    }
                }
            }
            NodeOrToken::Node(_) => {
                // A child node (e.g. BLOCK_ATTRIBUTE) before the function keyword — keep scanning.
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_fn_name() {
        assert_eq!(path_to_fn_name("baml.Array.length"), "baml_array_length");
        assert_eq!(path_to_fn_name("baml.deep_copy"), "baml_deep_copy");
        assert_eq!(path_to_fn_name("baml.math.trunc"), "baml_math_trunc");
        assert_eq!(path_to_fn_name("baml.media.Pdf.url"), "baml_media_pdf_url");
        assert_eq!(
            path_to_fn_name("baml.Array.push"),
            "baml_array_push"
        );
    }

    #[test]
    fn test_extract_native_builtins() {
        let builtins = extract_native_builtins();
        assert!(
            builtins.len() >= 24,
            "Expected at least 24 builtins, got {}",
            builtins.len()
        );

        // Spot-check: Array.length
        let array_length = builtins
            .iter()
            .find(|b| b.path == "baml.Array.length")
            .expect("missing Array.length");
        assert_eq!(array_length.fn_name, "baml_array_length");
        assert!(array_length.receiver.is_some());
        assert_eq!(array_length.params.len(), 0, "Array.length has no params besides self");

        // Spot-check: deep_copy (free function with generics)
        let deep_copy = builtins
            .iter()
            .find(|b| b.path == "baml.deep_copy")
            .expect("missing deep_copy");
        assert!(deep_copy.receiver.is_none());
        assert_eq!(deep_copy.generics, vec!["T"]);

        // Spot-check: Array.push has mut receiver
        let array_push = builtins
            .iter()
            .find(|b| b.path == "baml.Array.push")
            .expect("missing Array.push");
        assert!(
            array_push.receiver.as_ref().unwrap().is_mut,
            "Array.push should have mut receiver"
        );

        // Spot-check: String.length
        let string_length = builtins
            .iter()
            .find(|b| b.path == "baml.String.length")
            .expect("missing String.length");
        assert_eq!(string_length.fn_name, "baml_string_length");
        assert!(string_length.receiver.is_some());

        // Spot-check: math.trunc (namespaced free function)
        let math_trunc = builtins
            .iter()
            .find(|b| b.path == "baml.math.trunc")
            .expect("missing math.trunc");
        assert!(math_trunc.receiver.is_none());
        assert_eq!(math_trunc.params.len(), 1);
        assert!(matches!(math_trunc.params[0].ty, BamlType::Float));

        // Spot-check: media.Pdf.url (namespaced class method)
        let pdf_url = builtins
            .iter()
            .find(|b| b.path == "baml.media.Pdf.url")
            .expect("missing media.Pdf.url");
        assert!(pdf_url.receiver.is_some());
        assert_eq!(pdf_url.receiver.as_ref().unwrap().class_name, "Pdf");
    }
}
