//! Extract `$rust_function` and `$rust_io_function` builtins from the compiler2 `.baml` stdlib files.
//!
//! Iterates `baml_builtins2::ALL`, parses each file through the compiler2
//! front-end (lex → parse → lower), and collects every function whose body is
//! `FunctionBodyDef::Builtin(BuiltinKind::Vm)` or `BuiltinKind::Io` into a
//! `NativeBuiltin` record. The CST is also retained per file for
//! `//baml:mut_self`, `//baml:vm`, and `//baml:mut_vm` directive scanning.

use baml_base::FileId;
use baml_compiler_diagnostics::ToDiagnostic;
use baml_compiler_syntax::{NodeOrToken, SyntaxKind, SyntaxNode};
use baml_compiler2_ast::ast::{
    BuiltinKind, ClassDef, FunctionBodyDef, FunctionDef, Item, TypeExpr,
};

use crate::types::{
    BamlType, BuiltinPipeline, NativeBuiltin, NativeClassDef, NativeClassField, Param, Receiver,
    ReceiverType, VmUsage,
};

/// Convert a byte offset in source text to a 1-based `(line, column)` pair.
fn offset_to_line_col(source: &str, offset: u32) -> (usize, usize) {
    let offset = (offset as usize).min(source.len());
    let prefix = &source[..offset];
    let line = prefix.matches('\n').count() + 1;
    let col = offset - prefix.rfind('\n').map_or(0, |p| p + 1) + 1;
    (line, col)
}

/// Returned when a builtin `.baml` file has parse errors or HIR lowering diagnostics.
pub struct ExtractNativeBuiltinsError {
    message: String,
}

impl std::fmt::Debug for ExtractNativeBuiltinsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for ExtractNativeBuiltinsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExtractNativeBuiltinsError {}

/// Parse, lower, and extract all `$rust_function` and `$rust_io_function` builtins
/// from the `.baml` stdlib.
///
/// Returns `(vm_builtins, io_builtins, class_defs)`:
/// - `vm_builtins`: `$rust_function` builtins (synchronous, run inline in VM)
/// - `io_builtins`: `$rust_io_function` builtins (async, dispatched via engine)
/// - `class_defs`: class definitions with fields (for view/owned struct generation)
///
/// Fails with [`ExtractNativeBuiltinsError`] if any file has parse errors or non-empty HIR
/// diagnostics (so codegen never runs on a silently broken stdlib).
#[allow(clippy::type_complexity)]
pub fn extract_native_builtins()
-> Result<(Vec<NativeBuiltin>, Vec<NativeBuiltin>, Vec<NativeClassDef>), ExtractNativeBuiltinsError>
{
    let mut vm_builtins = Vec::new();
    let mut io_builtins = Vec::new();
    let mut class_defs = Vec::new();
    let mut diagnostic_lines: Vec<String> = Vec::new();

    for builtin_file in baml_builtins2::ALL
        .iter()
        .filter(|f| f.package == baml_builtins2::PACKAGE_BAML)
    {
        let path = builtin_file.virtual_path();
        // Real filesystem path for diagnostic messages (clickable in editors).
        let diag_path = format!(
            "{}/{}/{}",
            baml_builtins2::BAML_STD_DIR,
            builtin_file.package,
            builtin_file.relative_path
        );
        // Lex and parse into a lossless CST.
        let tokens = baml_compiler_lexer::lex_lossless(builtin_file.contents, FileId::new(0));
        let (green, errors) = baml_compiler_parser::parse_file(&tokens);
        for e in &errors {
            let d = e.to_diagnostic();
            let location = d
                .primary_span()
                .map(|span| {
                    let (line, col) =
                        offset_to_line_col(builtin_file.contents, span.range.start().into());
                    format!("{diag_path}:{line}:{col}")
                })
                .unwrap_or_else(|| diag_path.clone());
            diagnostic_lines.push(format!("  {location}: [{}] {}", d.id.code(), d.message));
        }
        if !errors.is_empty() {
            continue;
        }
        let cst_root = SyntaxNode::new_root(green);

        // Lower CST → AST items.
        let (items, diags, _) = baml_compiler2_ast::lower_file(&cst_root);
        for ld in &diags {
            let d = ld.to_diagnostic(FileId::new(0));
            let location = d
                .primary_span()
                .map(|span| {
                    let (line, col) =
                        offset_to_line_col(builtin_file.contents, span.range.start().into());
                    format!("{diag_path}:{line}:{col}")
                })
                .unwrap_or_else(|| diag_path.clone());
            diagnostic_lines.push(format!("  {location}: [{}] {}", d.id.code(), d.message));
        }
        if !diags.is_empty() {
            continue;
        }

        // Build the namespace prefix from the file's package and path-derived namespace.
        // e.g. package="baml", ns_path=["math"] → "baml.math"
        //      package="baml", ns_path=[]        → "baml"
        let ns_path = builtin_file.namespace_path();
        let namespace_prefix = if ns_path.is_empty() {
            builtin_file.package.to_string()
        } else {
            format!("{}.{}", builtin_file.package, ns_path.join("."))
        };

        for item in &items {
            match item {
                Item::Class(class_def) => {
                    extract_from_class(
                        class_def,
                        &namespace_prefix,
                        &cst_root,
                        &path,
                        &mut vm_builtins,
                        &mut io_builtins,
                    );
                    if let Some(class_def_record) =
                        extract_class_fields(class_def, &namespace_prefix, &path)
                    {
                        class_defs.push(class_def_record);
                    }
                }
                Item::Function(func_def) => {
                    extract_from_free_function(
                        func_def,
                        &namespace_prefix,
                        &cst_root,
                        &path,
                        &mut vm_builtins,
                        &mut io_builtins,
                    );
                }
                _ => {}
            }
        }
    }

    if !diagnostic_lines.is_empty() {
        return Err(ExtractNativeBuiltinsError {
            message: format!(
                "extract_native_builtins failed (fix stdlib .baml sources):\n{}",
                diagnostic_lines.join("\n")
            ),
        });
    }

    Ok((vm_builtins, io_builtins, class_defs))
}

/// Extract `$rust_function` and `$rust_io_function` methods from a class definition.
fn extract_from_class(
    class_def: &ClassDef,
    namespace_prefix: &str,
    cst_root: &SyntaxNode,
    source_file: &str,
    vm_builtins: &mut Vec<NativeBuiltin>,
    io_builtins: &mut Vec<NativeBuiltin>,
) {
    let class_name = class_def.name.as_str();
    let class_generics: Vec<String> = class_def
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();

    for method in &class_def.methods {
        let Some(pipeline) = extract_builtin_pipeline(method) else {
            continue;
        };

        // Merge class generics with method-level generics.
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

        let path = format!("{namespace_prefix}.{class_name}.{}", method.name.as_str());
        let fn_name = path_to_fn_name(&path);

        let has_self = method
            .params
            .first()
            .map(|p| p.name.as_str() == "self")
            .unwrap_or(false);

        let method_name = method.name.as_str();
        let is_mut =
            has_self && has_method_directive(cst_root, class_name, method_name, "//baml:mut_self");
        let has_vm = has_method_directive(cst_root, class_name, method_name, "//baml:vm");
        let has_mut_vm = has_method_directive(cst_root, class_name, method_name, "//baml:mut_vm");
        let may_yield = has_method_directive(cst_root, class_name, method_name, "//baml:may_yield");

        assert!(
            !(has_vm && has_mut_vm),
            "baml codegen error: {path} has both //baml:vm and //baml:mut_vm \
             -- these are mutually exclusive"
        );
        assert!(
            !(is_mut && (has_vm || has_mut_vm)),
            "baml codegen error: {path} has //baml:mut_self with //baml:vm or //baml:mut_vm \
             -- these are mutually exclusive (mutable receiver already borrows vm)"
        );
        assert!(
            !may_yield || has_mut_vm,
            "baml codegen error: {path} has //baml:may_yield without //baml:mut_vm \
             -- yielding methods require mutable VM access"
        );

        let vm_usage = if has_mut_vm {
            VmUsage::MutRef
        } else if has_vm {
            VmUsage::Ref
        } else {
            VmUsage::None
        };

        let throws = extract_throws(method);

        // Always set receiver for class methods — even static methods (no `self`)
        // need it for dispatch routing. The runtime path is
        // "baml.llm.StreamCache.new" which dispatches via class name.
        let receiver_type = if !has_self {
            ReceiverType::Static
        } else if is_mut {
            ReceiverType::MutSelf
        } else {
            ReceiverType::RefSelf
        };
        let receiver = Some(Receiver {
            class_name: class_name.to_string(),
            class_generics: class_generics.clone(),
            receiver_type,
        });
        let params = if has_self {
            extract_params_skip_self(method, &all_generics)
        } else {
            method
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
                .collect()
        };

        let return_type = method
            .return_type
            .as_ref()
            .map(|te| type_expr_to_baml_type(&te.expr, &all_generics))
            .unwrap_or(BamlType::Null);

        let builtin = NativeBuiltin {
            path,
            fn_name,
            params,
            return_type,
            generics: all_generics,
            receiver,
            vm_usage,
            may_yield,
            pipeline,
            throws,
            source_file: source_file.to_string(),
        };

        match pipeline {
            BuiltinPipeline::Vm => vm_builtins.push(builtin),
            BuiltinPipeline::Io => io_builtins.push(builtin),
        }
    }
}

/// Extract field definitions from a class, producing a `NativeClassDef`.
///
/// Returns `None` for classes that keep dedicated `Object` variants (Array, Map, String, `Uint8Array`)
/// since they don't use `Object::Instance`.
fn extract_class_fields(
    class_def: &ClassDef,
    namespace_prefix: &str,
    source_file: &str,
) -> Option<NativeClassDef> {
    let class_name = class_def.name.as_str();

    // Skip classes with dedicated Object variants — they are not Instance-based.
    match class_name {
        "Array" | "Map" | "String" | "Uint8Array" => return None,
        _ => {}
    }

    // Skip classes with no fields (pure namespace markers or method-only classes).
    if class_def.fields.is_empty() {
        return None;
    }

    let generic_params: Vec<String> = class_def
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();

    let fields: Vec<NativeClassField> = class_def
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_type = field
                .type_expr
                .as_ref()
                .map(|te| type_expr_to_baml_type(&te.expr, &generic_params))
                .unwrap_or(BamlType::Named("unknown".to_string()));
            NativeClassField {
                name: field.name.as_str().to_string(),
                field_type,
                index,
            }
        })
        .collect();

    Some(NativeClassDef {
        name: class_name.to_string(),
        namespace_prefix: namespace_prefix.to_string(),
        generic_params,
        fields,
        source_file: source_file.to_string(),
    })
}

/// Extract a `$rust_function` or `$rust_io_function` free function (not inside a class).
fn extract_from_free_function(
    func_def: &FunctionDef,
    namespace_prefix: &str,
    cst_root: &SyntaxNode,
    source_file: &str,
    vm_builtins: &mut Vec<NativeBuiltin>,
    io_builtins: &mut Vec<NativeBuiltin>,
) {
    let Some(pipeline) = extract_builtin_pipeline(func_def) else {
        return;
    };

    let generics: Vec<String> = func_def
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();

    let path = format!("{namespace_prefix}.{}", func_def.name.as_str());
    let fn_name = path_to_fn_name(&path);
    let has_vm = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:vm");
    let has_mut_vm = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:mut_vm");
    let may_yield = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:may_yield");

    assert!(
        !(has_vm && has_mut_vm),
        "baml codegen error: {path} has both //baml:vm and //baml:mut_vm \
         -- these are mutually exclusive"
    );
    assert!(
        !may_yield || has_mut_vm,
        "baml codegen error: {path} has //baml:may_yield without //baml:mut_vm \
         -- yielding functions require mutable VM access"
    );

    let vm_usage = if has_mut_vm {
        VmUsage::MutRef
    } else if has_vm {
        VmUsage::Ref
    } else {
        VmUsage::None
    };

    let throws = extract_throws(func_def);

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

    let builtin = NativeBuiltin {
        path,
        fn_name,
        params,
        return_type,
        generics,
        receiver: None,
        vm_usage,
        may_yield,
        pipeline,
        throws,
        source_file: source_file.to_string(),
    };

    match pipeline {
        BuiltinPipeline::Vm => vm_builtins.push(builtin),
        BuiltinPipeline::Io => io_builtins.push(builtin),
    }
}

/// Returns the pipeline kind if the function body is a Rust builtin, or None otherwise.
fn extract_builtin_pipeline(func: &FunctionDef) -> Option<BuiltinPipeline> {
    match &func.body {
        Some(FunctionBodyDef::Builtin(BuiltinKind::Vm)) => Some(BuiltinPipeline::Vm),
        Some(FunctionBodyDef::Builtin(BuiltinKind::Io)) => Some(BuiltinPipeline::Io),
        _ => None,
    }
}

/// Extract error categories from the `throws` clause of an IO function.
///
/// The `throws` field is `Option<SpannedTypeExpr>`. For a single error like
/// `throws root.errors.Io`, it's `TypeExpr::Path(["root", "errors", "Io"])`.
/// For multiple errors like `throws root.errors.Io | root.errors.Timeout`,
/// it's `TypeExpr::Union([Path(...), Path(...)])`.
fn extract_throws(func: &FunctionDef) -> Vec<String> {
    let Some(throws_expr) = &func.throws else {
        return vec![];
    };
    extract_throw_categories(&throws_expr.expr)
}

#[allow(clippy::redundant_closure_for_method_calls)]
fn extract_throw_categories(ty: &TypeExpr) -> Vec<String> {
    match ty {
        TypeExpr::Path { segments, .. } => {
            let path: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();
            if path.len() >= 3 && (path[0] == "baml" || path[0] == "root") && path[1] == "errors" {
                vec![path[2..].join(".")]
            } else {
                vec![
                    segments
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("."),
                ]
            }
        }
        TypeExpr::Union { variants, .. } => {
            variants.iter().flat_map(extract_throw_categories).collect()
        }
        _ => vec![],
    }
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
#[allow(clippy::redundant_closure_for_method_calls)]
fn type_expr_to_baml_type(ty: &TypeExpr, generics: &[String]) -> BamlType {
    match ty {
        TypeExpr::Int { .. } => BamlType::Int,
        TypeExpr::Float { .. } => BamlType::Float,
        TypeExpr::String { .. } => BamlType::String,
        TypeExpr::Bool { .. } => BamlType::Bool,
        TypeExpr::Null { .. } => BamlType::Null,
        TypeExpr::Never { .. } => BamlType::Null,
        TypeExpr::Void { .. } => BamlType::Null,

        TypeExpr::Media { kind, .. } => {
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

        TypeExpr::Uint8Array { .. } => BamlType::Uint8Array,

        TypeExpr::Optional { inner, .. } => {
            BamlType::Optional(Box::new(type_expr_to_baml_type(inner, generics)))
        }

        TypeExpr::List { inner, .. } => {
            BamlType::List(Box::new(type_expr_to_baml_type(inner, generics)))
        }

        TypeExpr::Map { key, value, .. } => BamlType::Map(
            Box::new(type_expr_to_baml_type(key, generics)),
            Box::new(type_expr_to_baml_type(value, generics)),
        ),

        TypeExpr::Path { segments, .. } => {
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

        TypeExpr::Union { variants, .. } => {
            let non_null: Vec<_> = variants
                .iter()
                .filter(|v| !matches!(v, TypeExpr::Null { .. }))
                .collect();
            if non_null.len() == 1 && non_null.len() < variants.len() {
                BamlType::Optional(Box::new(type_expr_to_baml_type(non_null[0], generics)))
            } else {
                BamlType::Named("union".to_string())
            }
        }
        TypeExpr::Literal { .. } => BamlType::Named("literal".to_string()),
        TypeExpr::Function { .. } => BamlType::Named("function".to_string()),
        TypeExpr::BuiltinUnknown { .. } | TypeExpr::Unknown { .. } | TypeExpr::Error { .. } => {
            BamlType::Named("unknown".to_string())
        }
        TypeExpr::Type { .. } => BamlType::Named("type".to_string()),
        TypeExpr::Rust { .. } => BamlType::RustType,
    }
}

/// Check if a method inside a class has the given `directive` comment (e.g. `"//baml:mut_self"`)
/// before its `function` keyword in the CST.
///
/// In the Rowan CST, the parser's `bump()` emits leading trivia tokens (whitespace,
/// comments) immediately before the `function` keyword inside the `FUNCTION_DEF` node
/// itself. So the directive appears as a `LINE_COMMENT` token child of the
/// `FUNCTION_DEF` node, before the `KW_FUNCTION` token.
fn has_method_directive(
    cst_root: &SyntaxNode,
    class_name: &str,
    method_name: &str,
    directive: &str,
) -> bool {
    for class_node in cst_root.descendants() {
        if class_node.kind() != SyntaxKind::CLASS_DEF {
            continue;
        }
        if !class_node_has_name(&class_node, class_name) {
            continue;
        }
        for func_node in class_node.descendants() {
            if func_node.kind() != SyntaxKind::FUNCTION_DEF {
                continue;
            }
            if !func_node_has_name(&func_node, method_name) {
                continue;
            }
            if function_node_has_leading_directive(&func_node, directive) {
                return true;
            }
        }
    }
    false
}

/// Check if a top-level (non-class) function has the given `directive` comment
/// before its `function` keyword in the CST.
fn has_free_fn_directive(cst_root: &SyntaxNode, fn_name: &str, directive: &str) -> bool {
    for node in cst_root.children() {
        if node.kind() != SyntaxKind::FUNCTION_DEF {
            continue;
        }
        if !func_node_has_name(&node, fn_name) {
            continue;
        }
        if function_node_has_leading_directive(&node, directive) {
            return true;
        }
    }
    false
}

/// Returns true if the `CLASS_DEF` node has a name token matching `class_name`.
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

/// Returns true if the `FUNCTION_DEF` node has a name matching `method_name`.
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

/// Check whether a `FUNCTION_DEF` node contains a specific directive `LINE_COMMENT`
/// (e.g. `"//baml:mut_self"` or `"//baml:mut_vm"`) before its `KW_FUNCTION` token.
///
/// The parser emits trivia (whitespace, comments) as tokens within the containing
/// syntactic node before the first real token. So any directive comment that
/// appears immediately before `function foo(...)` in source is stored as a
/// `LINE_COMMENT` child of that `FUNCTION_DEF` node.
fn function_node_has_leading_directive(func_node: &SyntaxNode, directive: &str) -> bool {
    for element in func_node.children_with_tokens() {
        match element {
            NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    let text = tok.text().trim();
                    if text == directive {
                        return true;
                    }
                }
                k if k.is_whitespace() => {}
                SyntaxKind::KW_FUNCTION => {
                    return false;
                }
                SyntaxKind::AT_AT | SyntaxKind::BLOCK_COMMENT => {}
                _ => {
                    return false;
                }
            },
            NodeOrToken::Node(_) => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_class_fields() {
        let (_vm, _io, class_defs) = extract_native_builtins().unwrap();

        let pdf = class_defs
            .iter()
            .find(|c| c.name == "Pdf")
            .expect("missing Pdf");
        assert_eq!(pdf.namespace_prefix, "baml.media");
        assert_eq!(pdf.fields.len(), 1);
        assert_eq!(pdf.fields[0].name, "_data");
        assert_eq!(pdf.fields[0].index, 0);
        assert!(matches!(pdf.fields[0].field_type, BamlType::RustType));

        assert!(
            class_defs.iter().any(|c| c.name == "Audio"),
            "missing Audio"
        );
        assert!(
            class_defs.iter().any(|c| c.name == "Video"),
            "missing Video"
        );
        assert!(
            class_defs.iter().any(|c| c.name == "Image"),
            "missing Image"
        );

        assert!(
            !class_defs.iter().any(|c| c.name == "Array"),
            "Array should be excluded"
        );
        assert!(
            !class_defs.iter().any(|c| c.name == "Map"),
            "Map should be excluded"
        );
        assert!(
            !class_defs.iter().any(|c| c.name == "String"),
            "String should be excluded"
        );

        // IO class fields
        let file = class_defs
            .iter()
            .find(|c| c.name == "File")
            .expect("missing File");
        assert_eq!(file.namespace_prefix, "baml.fs");
        assert_eq!(file.fields.len(), 1);
        assert_eq!(file.fields[0].name, "_handle");
        assert!(matches!(file.fields[0].field_type, BamlType::RustType));

        let socket = class_defs
            .iter()
            .find(|c| c.name == "Socket")
            .expect("missing Socket");
        assert_eq!(socket.namespace_prefix, "baml.net");
        assert_eq!(socket.fields.len(), 1);
        assert_eq!(socket.fields[0].name, "_handle");
        assert!(matches!(socket.fields[0].field_type, BamlType::RustType));

        let response = class_defs
            .iter()
            .find(|c| c.name == "Response")
            .expect("missing Response");
        assert_eq!(response.namespace_prefix, "baml.http");
        assert_eq!(response.fields.len(), 4);
        assert_eq!(response.fields[0].name, "status_code");
        assert!(matches!(response.fields[0].field_type, BamlType::Int));
        assert_eq!(response.fields[1].name, "headers");
        assert!(matches!(response.fields[1].field_type, BamlType::Map(_, _)));
        assert_eq!(response.fields[2].name, "url");
        assert!(matches!(response.fields[2].field_type, BamlType::String));
        assert_eq!(response.fields[3].name, "_body");
        assert!(matches!(response.fields[3].field_type, BamlType::RustType));

        let request = class_defs
            .iter()
            .find(|c| c.name == "Request")
            .expect("missing Request");
        assert_eq!(request.namespace_prefix, "baml.http");
        assert_eq!(request.fields.len(), 4);

        // LLM classes
        let pc = class_defs
            .iter()
            .find(|c| c.name == "PrimitiveClient")
            .expect("missing PrimitiveClient");
        assert_eq!(pc.namespace_prefix, "baml.llm");

        let client = class_defs
            .iter()
            .find(|c| c.name == "Client")
            .expect("missing Client");
        assert_eq!(client.namespace_prefix, "baml.llm");

        let retry = class_defs
            .iter()
            .find(|c| c.name == "RetryPolicy")
            .expect("missing RetryPolicy");
        assert_eq!(retry.namespace_prefix, "baml.llm");
    }

    #[test]
    fn test_path_to_fn_name() {
        assert_eq!(path_to_fn_name("baml.Array.length"), "baml_array_length");
        assert_eq!(path_to_fn_name("baml.deep_copy"), "baml_deep_copy");
        assert_eq!(path_to_fn_name("baml.math.trunc"), "baml_math_trunc");
        assert_eq!(path_to_fn_name("baml.media.Pdf.url"), "baml_media_pdf_url");
        assert_eq!(path_to_fn_name("baml.Array.push"), "baml_array_push");
    }

    #[test]
    fn test_sys_op_variant_name() {
        let make = |path: &str| NativeBuiltin {
            path: path.to_string(),
            fn_name: String::new(),
            params: vec![],
            return_type: BamlType::Null,
            generics: vec![],
            receiver: None,
            vm_usage: VmUsage::None,
            may_yield: false,
            pipeline: BuiltinPipeline::Io,
            throws: vec![],
            source_file: String::new(),
        };
        assert_eq!(make("baml.fs.open").sys_op_variant_name(), "BamlFsOpen");
        assert_eq!(
            make("baml.fs.File.read").sys_op_variant_name(),
            "BamlFsFileRead"
        );
        assert_eq!(make("baml.env.get").sys_op_variant_name(), "BamlEnvGet");
        assert_eq!(
            make("baml.http.fetch").sys_op_variant_name(),
            "BamlHttpFetch"
        );
        assert_eq!(make("baml.sys.panic").sys_op_variant_name(), "BamlSysPanic");
        assert_eq!(
            make("baml.llm.PrimitiveClient.render_prompt").sys_op_variant_name(),
            "BamlLlmPrimitiveClientRenderPrompt"
        );
        assert_eq!(
            make("baml.llm.get_jinja_template").sys_op_variant_name(),
            "BamlLlmGetJinjaTemplate"
        );
    }

    #[test]
    fn test_extract_vm_builtins_unchanged() {
        let (vm_builtins, _io, _class_defs) = extract_native_builtins().unwrap();
        assert!(
            vm_builtins.len() >= 24,
            "Expected at least 24 VM builtins, got {}",
            vm_builtins.len()
        );

        // All VM builtins should have pipeline == Vm. They MAY declare a `throws`
        // clause (e.g. `Array.map<U, E>(... throws E)` carries `E` through, and
        // `Uint8Array.from_hex` throws `InvalidArgument`). Codegen consumes the
        // declared throws to decide whether the trait method returns a `Result`.
        for b in &vm_builtins {
            assert_eq!(b.pipeline, BuiltinPipeline::Vm, "{} should be Vm", b.path);
        }

        let array_length = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Array.length")
            .expect("missing Array.length");
        assert_eq!(array_length.fn_name, "baml_array_length");
        assert!(array_length.receiver.is_some());
        assert_eq!(array_length.params.len(), 0);
        // Non-throwing builtin must have empty throws (otherwise codegen would
        // wrap it in a spurious `Result`).
        assert!(array_length.throws.is_empty());

        // Concrete-error throws: `Uint8Array.from_hex` rejects malformed input
        // with `InvalidArgument`. Pin this so a regression in throws extraction
        // (or in the .baml signature) trips this test instead of silently
        // dropping the `Result` wrapper from the generated trait method.
        let from_hex = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Uint8Array.from_hex")
            .expect("missing Uint8Array.from_hex");
        assert_eq!(from_hex.throws, vec!["InvalidArgument"]);

        // Generic-throws: `Array.map<U, E>(... throws E)` carries the callback's
        // error type through. The extractor records the generic name verbatim.
        let array_map = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Array.map")
            .expect("missing Array.map");
        assert_eq!(array_map.throws, vec!["E"]);

        let deep_copy = vm_builtins
            .iter()
            .find(|b| b.path == "baml.deep_copy")
            .expect("missing deep_copy");
        assert!(deep_copy.receiver.is_none());
        assert_eq!(deep_copy.generics, vec!["T"]);

        let array_push = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Array.push")
            .expect("missing Array.push");
        assert!(array_push.receiver.as_ref().unwrap().receiver_type.is_mut());

        let string_length = vm_builtins
            .iter()
            .find(|b| b.path == "baml.String.length")
            .expect("missing String.length");
        assert_eq!(string_length.fn_name, "baml_string_length");

        let math_trunc = vm_builtins
            .iter()
            .find(|b| b.path == "baml.math.trunc")
            .expect("missing math.trunc");
        assert!(math_trunc.receiver.is_none());
        assert_eq!(math_trunc.params.len(), 1);
        assert!(matches!(math_trunc.params[0].ty, BamlType::Float));

        let pdf_url = vm_builtins
            .iter()
            .find(|b| b.path == "baml.media.Pdf.url")
            .expect("missing media.Pdf.url");
        assert!(pdf_url.receiver.is_some());
        assert_eq!(pdf_url.receiver.as_ref().unwrap().class_name, "Pdf");

        assert_eq!(deep_copy.vm_usage, VmUsage::MutRef);

        let deep_equals = vm_builtins
            .iter()
            .find(|b| b.path == "baml.deep_equals")
            .expect("missing deep_equals");
        assert_eq!(deep_equals.vm_usage, VmUsage::Ref);

        assert_eq!(array_length.vm_usage, VmUsage::None);
        assert_eq!(array_push.vm_usage, VmUsage::None);
        assert_eq!(math_trunc.vm_usage, VmUsage::None);

        let string_split = vm_builtins
            .iter()
            .find(|b| b.path == "baml.String.split")
            .expect("missing String.split");
        assert_eq!(string_split.vm_usage, VmUsage::None);
    }

    #[test]
    fn test_io_builtin_throws() {
        let (_vm, io_builtins, _class_defs) = extract_native_builtins().unwrap();

        let fs_open = io_builtins
            .iter()
            .find(|b| b.path == "baml.fs.open")
            .unwrap();
        assert_eq!(fs_open.throws, vec!["Io"]);

        let net_connect = io_builtins
            .iter()
            .find(|b| b.path == "baml.net.connect")
            .unwrap();
        assert_eq!(net_connect.throws, vec!["Io", "Timeout"]);

        let http_fetch = io_builtins
            .iter()
            .find(|b| b.path == "baml.http.fetch")
            .unwrap();
        assert_eq!(http_fetch.throws, vec!["Io", "Timeout"]);

        let render_prompt = io_builtins
            .iter()
            .find(|b| b.path == "baml.llm.PrimitiveClient.render_prompt")
            .unwrap();
        assert_eq!(render_prompt.throws, vec!["RenderPrompt"]);

        let specialize = io_builtins
            .iter()
            .find(|b| b.path == "baml.llm.PrimitiveClient.specialize_prompt")
            .unwrap();
        assert_eq!(specialize.throws, vec!["RenderPrompt", "LlmClient"]);
    }
}
