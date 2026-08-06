//! Direct-import projection used by editor and bundler tooling.
//!
//! This lives beside the SDK emitter so every TypeScript surface consumes the
//! same `SymbolPool` and, critically, the same structural `Ty` translator.
//! The tooling host supplies the compiler export document only for stable
//! symbol ids and source-map decoration; it is never used to reconstruct a
//! type from display text.

use std::{collections::BTreeSet, fmt::Write as _, str::FromStr};

use baml_codegen_types::{Name, Symbol, SymbolPool, Ty};
use baml_surface::{IdKind, PackageExport, SymbolId, export::ItemDetail};

use crate::{
    routing::route,
    translate_ty::{TranslateCtx, translate_ty},
    ts_string,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolingDeclarationRole {
    Declaration,
    Type,
    Documentation,
    Synthetic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolingMappedSpan {
    pub start_utf16: u32,
    pub length_utf16: u32,
    pub symbol_id: String,
    pub role: ToolingDeclarationRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolingEmitOutput {
    pub declaration: String,
    pub code: String,
    pub spans: Vec<ToolingMappedSpan>,
    pub type_exports: BTreeSet<String>,
    pub value_exports: BTreeSet<String>,
}

struct Writer {
    text: String,
    spans: Vec<ToolingMappedSpan>,
}

impl Writer {
    fn push(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn mapped(&mut self, text: &str, symbol_id: &str, role: ToolingDeclarationRole) {
        let start_utf16 = u32::try_from(self.text.encode_utf16().count())
            .expect("generated TypeScript declaration exceeds u32 UTF-16 offsets");
        self.text.push_str(text);
        self.spans.push(ToolingMappedSpan {
            start_utf16,
            length_utf16: u32::try_from(text.encode_utf16().count())
                .expect("generated TypeScript segment exceeds u32 UTF-16 offsets"),
            symbol_id: symbol_id.to_string(),
            role,
        });
    }

    fn mapped_type(&mut self, ty: &Ty, current: &baml_codegen_types::Name, head: Option<&str>) {
        let translated = translate_ty(
            ty,
            &TranslateCtx {
                current_leaf: route(current),
            },
        );
        if let Some(symbol_id) = head {
            self.mapped(&translated.expr, symbol_id, ToolingDeclarationRole::Type);
        } else {
            self.push(&translated.expr);
        }
    }
}

/// Resolve a compiler export item to its `SymbolPool` entry by stable symbol
/// identity — package, namespace path, name, and lookup space — never by bare
/// display name. Duplicate names in different namespaces (and same-named
/// type/value pairs, which BAML allows) would otherwise silently select the
/// wrong symbol and emit the wrong field, argument, or return types.
fn pool_entry<'a>(pool: &'a SymbolPool, id: &str) -> Option<(&'a Name, &'a Symbol)> {
    let id = SymbolId::from_str(id).ok()?;
    pool.iter().find(|(name, symbol)| {
        name.package().as_str() == id.package
            && name
                .namespace()
                .iter()
                .map(smol_str::SmolStr::as_str)
                .eq(id.namespace.iter().map(String::as_str))
            && name.name().as_str() == id.name
            && matches!(
                (symbol, id.kind),
                (
                    Symbol::Class(_) | Symbol::Enum(_) | Symbol::TypeAlias(_),
                    IdKind::Type
                ) | (Symbol::Function(_), IdKind::Value)
            )
    })
}

/// Emit the declaration and JavaScript projections for a direct `.baml`
/// import. Type expressions are translated from compiler-owned `Ty` values.
pub fn emit_tooling_module(
    pool: &SymbolPool,
    exports: &PackageExport,
    runtime_id: &str,
    runtime_package: &str,
    banner: &str,
) -> ToolingEmitOutput {
    let mut out = Writer {
        text: banner.to_string(),
        spans: Vec::new(),
    };
    let mut type_exports = BTreeSet::new();
    let mut value_exports = BTreeSet::new();

    for item in &exports.items {
        let symbol = pool_entry(pool, &item.id);
        if let Some(doc) = &item.docstring {
            let doc = format!("/** {} */", escape_doc(doc));
            out.mapped(&doc, &item.id, ToolingDeclarationRole::Documentation);
            out.push("\n");
        }
        match (&item.detail, symbol) {
            (ItemDetail::Class { fields, .. }, Some((name, Symbol::Class(class)))) => {
                type_exports.insert(item.name.clone());
                out.push("export interface ");
                out.mapped(&item.name, &item.id, declaration_role(item.synthetic));
                out.push(" {\n");
                for field in fields {
                    let Some(property) = class
                        .properties
                        .iter()
                        .find(|p| p.name.as_str() == field.name)
                    else {
                        continue;
                    };
                    if let Some(doc) = &field.docstring {
                        let _ = writeln!(out.text, "  /** {} */", escape_doc(doc));
                    }
                    out.push("  ");
                    out.mapped(&field.name, &field.id, ToolingDeclarationRole::Declaration);
                    out.push(": ");
                    out.mapped_type(&property.ty, name, field.ty.head.as_deref());
                    out.push(";\n");
                }
                out.push("}\n\n");
            }
            (ItemDetail::Interface { .. }, _) => {
                type_exports.insert(item.name.clone());
                out.push("export interface ");
                out.mapped(&item.name, &item.id, declaration_role(item.synthetic));
                out.push(" {}\n\n");
            }
            (ItemDetail::Enum { variants, .. }, Some((_, Symbol::Enum(enm)))) => {
                type_exports.insert(item.name.clone());
                value_exports.insert(item.name.clone());
                out.push("export enum ");
                out.mapped(&item.name, &item.id, declaration_role(item.synthetic));
                out.push(" {\n");
                for variant in variants {
                    out.push("  ");
                    out.mapped(
                        &variant.name,
                        &variant.id,
                        ToolingDeclarationRole::Declaration,
                    );
                    let value = enm
                        .variants
                        .iter()
                        .find(|v| v.name.as_str() == variant.name)
                        .map_or(variant.name.as_str(), |v| v.value.as_str());
                    let _ = writeln!(out.text, " = {},", ts_string(value));
                }
                out.push("}\n\n");
            }
            (ItemDetail::TypeAlias { resolved }, Some((name, Symbol::TypeAlias(alias)))) => {
                type_exports.insert(item.name.clone());
                out.push("export type ");
                out.mapped(&item.name, &item.id, declaration_role(item.synthetic));
                out.push(" = ");
                out.mapped_type(&alias.resolves_to, name, resolved.head.as_deref());
                out.push(";\n\n");
            }
            (ItemDetail::Function { .. } | ItemDetail::Plain {}, _) => {}
            // A compiler export and codegen pool should agree. Keeping the
            // unmatched arm non-panicking lets diagnostics remain available
            // during compiler evolution, but emits no guessed declaration.
            _ => {}
        }
    }

    type_exports.insert("BamlClient".to_string());
    value_exports.insert("b".to_string());
    out.push("export interface BamlClient {\n");
    for item in &exports.items {
        let ItemDetail::Function { signature } = &item.detail else {
            continue;
        };
        let Some((name, Symbol::Function(function))) = pool_entry(pool, &item.id) else {
            continue;
        };
        // The export document and the pool are produced by the same compiler
        // revision, so the counts agree; if they ever drift, skip the function
        // rather than silently truncating the longer parameter list.
        if signature.params.len() != function.arguments.len() {
            continue;
        }
        out.push("  readonly ");
        out.mapped(&item.name, &item.id, declaration_role(item.synthetic));
        out.push(": (");
        for (index, (param, argument)) in
            signature.params.iter().zip(&function.arguments).enumerate()
        {
            if index > 0 {
                out.push(", ");
            }
            out.push(&param.name);
            if param.optional {
                out.push("?");
            }
            out.push(": ");
            out.mapped_type(&argument.ty, name, param.ty.head.as_deref());
        }
        out.push(") => Promise<");
        out.mapped_type(
            &function.return_type,
            name,
            signature.returns.head.as_deref(),
        );
        out.push(">;\n");
    }
    out.push("}\nexport declare const b: BamlClient;\n");

    let uses_runtime_types = out.text.contains("baml.media.")
        || out.text.contains("baml.llm.")
        || out.text.contains("_BamlHandle");
    if uses_runtime_types {
        // Materialize the canonical translator's stdlib namespace references
        // as aliases to the actual runtime-owned values. Direct imports and
        // generated SDKs therefore expose the same media/stream identities.
        let preamble = format!(
            "import type {{ BamlAudio as __BamlAudio, BamlHandle as _BamlHandle, BamlImage as __BamlImage, BamlPdf as __BamlPdf, BamlStream as __BamlStream, BamlVideo as __BamlVideo }} from {};\n\
             declare namespace baml {{ namespace media {{ type Image = __BamlImage; type Audio = __BamlAudio; type Video = __BamlVideo; type Pdf = __BamlPdf; }} namespace llm {{ type Stream<TStream, TFinal = TStream> = __BamlStream<TStream, TFinal>; }} }}\n\n",
            ts_string(runtime_package)
        );
        let delta = u32::try_from(preamble.encode_utf16().count())
            .expect("generated TypeScript preamble exceeds u32 UTF-16 offsets");
        out.text.insert_str(banner.len(), &preamble);
        for span in &mut out.spans {
            span.start_utf16 = span
                .start_utf16
                .checked_add(delta)
                .expect("generated TypeScript declaration exceeds u32 UTF-16 offsets");
        }
    }

    let mut code = format!(
        "import {{ defineFunction }} from {};\n",
        ts_string(runtime_id)
    );
    for item in &exports.items {
        let ItemDetail::Enum { variants, .. } = &item.detail else {
            continue;
        };
        let _ = writeln!(code, "export const {} = {{", item.name);
        for variant in variants {
            let _ = writeln!(code, "  {}: {},", variant.name, ts_string(&variant.name));
        }
        code.push_str("};\n");
    }
    code.push_str("export const b = {\n");
    for item in &exports.items {
        let ItemDetail::Function { signature } = &item.detail else {
            continue;
        };
        let params = signature
            .params
            .iter()
            .map(|param| ts_string(&param.name))
            .collect::<Vec<_>>()
            .join(", ");
        let fqn = item
            .id
            .split_once(':')
            .map_or(item.id.as_str(), |(_, fqn)| fqn);
        let _ = writeln!(
            code,
            "  {0}: defineFunction({1}, \"async\", [{params}]),",
            item.name,
            ts_string(fqn)
        );
    }
    code.push_str("};\n");

    ToolingEmitOutput {
        declaration: out.text,
        code,
        spans: out.spans,
        type_exports,
        value_exports,
    }
}

fn declaration_role(synthetic: bool) -> ToolingDeclarationRole {
    if synthetic {
        ToolingDeclarationRole::Synthetic
    } else {
        ToolingDeclarationRole::Declaration
    }
}

fn escape_doc(doc: &str) -> String {
    doc.replace("*/", "*\\/").replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{Class, ClassProperty, Enum, EnumVariant, Function, Origin};
    use baml_surface::{
        SymbolKind,
        export::{
            FieldExport, ItemExport, PackageExport, ParamExport, SignatureExport, SourceExport,
            ThrowsExport, TyRef, VariantExport,
        },
    };

    use super::*;

    fn name(pkg: &str, namespace: &[&str], short: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            namespace.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(short),
        )
    }

    fn origin() -> Origin {
        Origin {
            source_file_path: "main.baml".to_string(),
            span_start: 0,
        }
    }

    fn class_with_field(pool_name: &Name, field: &str, ty: Ty) -> Symbol {
        Symbol::Class(Class {
            name: pool_name.clone(),
            generic_params: Vec::new(),
            docstring: None,
            properties: vec![ClassProperty {
                name: BaseName::new(field),
                docstring: None,
                ty,
            }],
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: origin(),
        })
    }

    fn int_ty() -> Ty {
        Ty::Int {
            attr: baml_base::TyAttr::EMPTY,
        }
    }

    fn string_ty() -> Ty {
        Ty::String {
            attr: baml_base::TyAttr::EMPTY,
        }
    }

    fn ty_ref(display: &str) -> TyRef {
        TyRef {
            display: display.to_string(),
            head: None,
            unresolved: false,
        }
    }

    fn source() -> SourceExport {
        SourceExport {
            file: "main.baml".to_string(),
            start: 0,
            end: 0,
        }
    }

    fn throws() -> ThrowsExport {
        ThrowsExport {
            declared: None,
            effective: ty_ref("never"),
            panics: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn export_of(items: Vec<ItemExport>) -> PackageExport {
        PackageExport {
            format_version: 1,
            package: "user".to_string(),
            items,
            impls: Vec::new(),
        }
    }

    #[test]
    fn symbol_lookup_uses_identity_not_bare_display_name() {
        // Two namespaces legitimately declare the same display name; joining
        // on `item.name` would pick whichever the pool hashes first.
        let mut pool = SymbolPool::new();
        let ns1 = name("user", &["one"], "Widget");
        let ns2 = name("user", &["two"], "Widget");
        pool.insert(ns1.clone(), class_with_field(&ns1, "a", int_ty()));
        pool.insert(ns2.clone(), class_with_field(&ns2, "b", string_ty()));

        let export = export_of(vec![ItemExport {
            id: "T:user.two.Widget".to_string(),
            kind: SymbolKind::Class,
            name: "Widget".to_string(),
            namespace: vec!["two".to_string()],
            docstring: None,
            synthetic: false,
            source: source(),
            detail: ItemDetail::Class {
                generics: Vec::new(),
                fields: vec![FieldExport {
                    id: "F:user.two.Widget.b".to_string(),
                    name: "b".to_string(),
                    docstring: None,
                    ty: ty_ref("string"),
                }],
                methods: Vec::new(),
                impls: Vec::new(),
            },
        }]);
        let emitted = emit_tooling_module(&pool, &export, "rt", "@boundaryml/baml-bridge", "");
        assert!(emitted.declaration.contains("b: string;"));
        assert!(!emitted.declaration.contains("a: number;"));
    }

    #[test]
    fn type_and_value_namespaces_may_share_a_display_name() {
        // `class Foo` and `function Foo` coexist in BAML; the kind
        // discriminant keeps their pool entries distinct.
        let mut pool = SymbolPool::new();
        let class_name = name("user", &[], "Foo");
        pool.insert(
            class_name.clone(),
            class_with_field(&class_name, "x", int_ty()),
        );
        let function_name = name("user", &[], "Foo");
        pool.insert(
            function_name,
            Symbol::Function(Function {
                name: BaseName::new("Foo"),
                generic_params: Vec::new(),
                docstring: None,
                arguments: Vec::new(),
                return_type: int_ty(),
                throws: None,
                watchers: Vec::new(),
                origin: origin(),
            }),
        );
        let export = export_of(vec![ItemExport {
            id: "V:user.Foo".to_string(),
            kind: SymbolKind::Function,
            name: "Foo".to_string(),
            namespace: Vec::new(),
            docstring: None,
            synthetic: false,
            source: source(),
            detail: ItemDetail::Function {
                signature: SignatureExport {
                    generics: Vec::new(),
                    params: Vec::new(),
                    returns: ty_ref("int"),
                    throws: throws(),
                },
            },
        }]);
        let emitted = emit_tooling_module(&pool, &export, "rt", "@boundaryml/baml-bridge", "");
        assert!(
            emitted
                .declaration
                .contains("readonly Foo: () => Promise<number>;")
        );
    }

    #[test]
    fn drifting_param_count_is_skipped_not_truncated() {
        let mut pool = SymbolPool::new();
        let function_name = name("user", &[], "Read");
        pool.insert(
            function_name,
            Symbol::Function(Function {
                name: BaseName::new("Read"),
                generic_params: Vec::new(),
                docstring: None,
                arguments: vec![baml_codegen_types::FunctionArgument {
                    name: BaseName::new("only"),
                    docstring: None,
                    ty: int_ty(),
                    default: None,
                }],
                return_type: int_ty(),
                throws: None,
                watchers: Vec::new(),
                origin: origin(),
            }),
        );
        let export = export_of(vec![ItemExport {
            id: "V:user.Read".to_string(),
            kind: SymbolKind::Function,
            name: "Read".to_string(),
            namespace: Vec::new(),
            docstring: None,
            synthetic: false,
            source: source(),
            detail: ItemDetail::Function {
                signature: SignatureExport {
                    generics: Vec::new(),
                    params: vec![
                        ParamExport {
                            name: "first".to_string(),
                            ty: ty_ref("int"),
                            optional: false,
                        },
                        ParamExport {
                            name: "second".to_string(),
                            ty: ty_ref("int"),
                            optional: false,
                        },
                    ],
                    returns: ty_ref("int"),
                    throws: throws(),
                },
            },
        }]);
        let emitted = emit_tooling_module(&pool, &export, "rt", "@boundaryml/baml-bridge", "");
        assert!(
            !emitted.declaration.contains("readonly Read:"),
            "a drifting signature must not emit a truncated parameter list: {}",
            emitted.declaration
        );
    }

    #[test]
    fn emitted_strings_are_valid_javascript() {
        // Rust's `{:?}` renders control characters as `\u{1}`, which is a
        // syntax error in JavaScript; the emitter must use JS escapes.
        let mut pool = SymbolPool::new();
        let enum_name = name("user", &[], "Level");
        pool.insert(
            enum_name.clone(),
            Symbol::Enum(Enum {
                name: enum_name,
                docstring: None,
                variants: vec![EnumVariant {
                    name: BaseName::new("Low"),
                    docstring: None,
                    value: "low\u{1}fire".to_string(),
                }],
                origin: origin(),
            }),
        );
        let export = export_of(vec![ItemExport {
            id: "T:user.Level".to_string(),
            kind: SymbolKind::Enum,
            name: "Level".to_string(),
            namespace: Vec::new(),
            docstring: None,
            synthetic: false,
            source: source(),
            detail: ItemDetail::Enum {
                variants: vec![VariantExport {
                    id: "E:user.Level.Low".to_string(),
                    name: "Low".to_string(),
                    docstring: None,
                }],
                impls: Vec::new(),
            },
        }]);
        let emitted = emit_tooling_module(&pool, &export, "rt", "@boundaryml/baml-bridge", "");
        assert!(!emitted.declaration.contains("\\u{"));
        assert!(!emitted.code.contains("\\u{"));
        assert!(emitted.declaration.contains("Low = \"low\\x01fire\""));
        assert!(emitted.code.contains("Low: \"Low\""));
    }

    #[test]
    fn segment_map_spans_track_generated_offsets() {
        let mut pool = SymbolPool::new();
        let class_name = name("user", &[], "Bag");
        pool.insert(
            class_name.clone(),
            class_with_field(&class_name, "scores", int_ty()),
        );
        let export = export_of(vec![ItemExport {
            id: "T:user.Bag".to_string(),
            kind: SymbolKind::Class,
            name: "Bag".to_string(),
            namespace: Vec::new(),
            docstring: Some("A bag.".to_string()),
            synthetic: false,
            source: source(),
            detail: ItemDetail::Class {
                generics: Vec::new(),
                fields: vec![FieldExport {
                    id: "F:user.Bag.scores".to_string(),
                    name: "scores".to_string(),
                    docstring: None,
                    ty: ty_ref("int"),
                }],
                methods: Vec::new(),
                impls: Vec::new(),
            },
        }]);
        let emitted = emit_tooling_module(&pool, &export, "rt", "@boundaryml/baml-bridge", "");
        for span in &emitted.spans {
            let text: String = emitted
                .declaration
                .chars()
                .skip(span.start_utf16 as usize)
                .take(span.length_utf16 as usize)
                .collect();
            match span.symbol_id.as_str() {
                "T:user.Bag" => assert!(text == "Bag" || text == "/** A bag. */"),
                "F:user.Bag.scores" => assert_eq!(text, "scores"),
                other => panic!("unexpected span for {other}"),
            }
        }
        assert!(
            emitted
                .spans
                .iter()
                .any(|span| span.symbol_id == "F:user.Bag.scores")
        );
    }
}
