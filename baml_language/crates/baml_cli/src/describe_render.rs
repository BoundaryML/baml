//! Handle-based text rendering for `baml describe` — declarations
//! reconstructed from the semantic surface rather than sliced from source.
//!
//! Layout, per item: a header line (kind + qualified name + location), the
//! docstring, the declaration shape, then members — and for types, the
//! rustdoc-style `implements` section grouped by interface, which the legacy
//! source-slicing renderer could not see at all.

use std::fmt::Write as _;

use baml_project::ProjectDatabase;
use baml_surface::{
    Class, Db, Enum, Function, Impl, Interface, Member, Symbol, TyDisplayFormat, TypeAlias,
};

/// Soft cap: how many docstring lines to show before eliding.
const DOC_LINES: usize = 12;

fn ty(t: &baml_type::Ty) -> String {
    TyDisplayFormat::UserFacing.render(t)
}

fn location(db: &ProjectDatabase, file: baml_db::SourceFile, span: text_size::TextRange) -> String {
    let text = file.text(db);
    let line = text[..usize::from(span.start()).min(text.len())]
        .matches('\n')
        .count()
        + 1;
    format!("{}:{line}", file.path(db).to_string_lossy())
}

fn push_docstring(out: &mut String, doc: Option<&str>) {
    let Some(doc) = doc else { return };
    let lines: Vec<&str> = doc.lines().collect();
    for line in lines.iter().take(DOC_LINES) {
        if line.is_empty() {
            let _ = writeln!(out, "  ///");
        } else {
            let _ = writeln!(out, "  /// {line}");
        }
    }
    if lines.len() > DOC_LINES {
        let _ = writeln!(out, "  /// … ({} more lines)", lines.len() - DOC_LINES);
    }
}

fn render_generics(params: &[(baml_type::ParamTy, Vec<baml_type::Interface>)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = params
        .iter()
        .map(|(param, bounds)| {
            if bounds.is_empty() {
                param.as_str().to_string()
            } else {
                let names: Vec<String> =
                    bounds.iter().map(|b| b.name.render_user_facing()).collect();
                format!("{} extends {}", param.as_str(), names.join(" & "))
            }
        })
        .collect();
    format!("<{}>", parts.join(", "))
}

/// One-line function signature in BAML syntax, from the resolved facts.
fn render_signature(db: &dyn Db, function: Function<'_>) -> String {
    let sig = function.signature(db);
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| {
            let name = p.name.as_ref().map_or("_", |n| n.as_str());
            if name == "self" {
                "self".to_string()
            } else {
                let mut s = format!("{name}: {}", ty(&p.ty));
                if matches!(p.mode, baml_type::FunctionParamMode::Optional) {
                    s.push_str(" = …");
                }
                s
            }
        })
        .collect();
    let generics = render_generics(&function.generic_params(db));
    let mut line = format!(
        "function {}{generics}({}) -> {}",
        function.name(db),
        params.join(", "),
        ty(&sig.return_type)
    );
    let throws = function.throws(db);
    if !matches!(throws.effective, baml_type::Ty::Never { .. }) {
        let _ = write!(line, " throws {}", ty(&throws.effective));
    }
    line
}

fn push_method_list(out: &mut String, db: &dyn Db, header: &str, methods: &[Function<'_>]) {
    if methods.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{header}:");
    for method in methods {
        let _ = writeln!(out, "  {}", render_signature(db, *method));
        if let Some(first) = method.docstring(db).and_then(|d| d.lines().next()) {
            let _ = writeln!(out, "      {first}");
        }
    }
}

fn push_impls(out: &mut String, db: &ProjectDatabase, impls: &[Impl<'_>], allows_any_class: bool) {
    if impls.is_empty() {
        return;
    }
    // A blanket impl (`implements<T> I for T`) attaches to every type; a full
    // entry per type is noise. Collapse them to one line each, rustdoc-style.
    let (blankets, direct): (Vec<&Impl<'_>>, Vec<&Impl<'_>>) = impls.iter().partition(|imp| {
        imp.for_ty(db)
            .and_then(baml_surface::ty_head)
            .is_some_and(|head| head == baml_surface::TyHead::Blanket)
    });
    if !direct.is_empty() {
        let _ = writeln!(out, "\nimplements:");
    }
    for imp in direct {
        let Some(iface) = imp.interface(db) else {
            continue;
        };
        let generics: Vec<String> = imp
            .generic_params(db)
            .unwrap_or_default()
            .iter()
            .map(|(param, bounds)| {
                if bounds.is_empty() {
                    param.as_str().to_string()
                } else {
                    let names: Vec<String> =
                        bounds.iter().map(|b| b.name.render_user_facing()).collect();
                    format!("{} extends {}", param.as_str(), names.join(" & "))
                }
            })
            .collect();
        let generics = if generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", generics.join(", "))
        };
        let for_ty = imp.for_ty(db).map(ty).unwrap_or_else(|| "?".to_string());
        let _ = writeln!(
            out,
            "  implements{generics} {} for {for_ty}   ({})",
            iface.qualified_name(db).render_user_facing(),
            location(db, imp.file(db), imp.span(db)),
        );
        for (name, bound_ty) in imp.assoc_bindings(db).unwrap_or_default() {
            let _ = writeln!(out, "    type {name} = {}", ty(bound_ty));
        }
        for method in imp.all_methods(db) {
            let suffix = if method.from_default {
                "   (default)"
            } else {
                ""
            };
            let _ = writeln!(out, "    {}{suffix}", render_signature(db, method.function));
        }
    }
    let blankets: Vec<_> = blankets
        .into_iter()
        .filter(|imp| {
            allows_any_class
                || imp
                    .interface(db)
                    .is_none_or(|iface| !iface.qualified_name(db).is_reflect_root_type("AnyClass"))
        })
        .collect();
    if !blankets.is_empty() {
        let _ = writeln!(out, "\nblanket implementations:");
        for imp in blankets {
            let Some(iface) = imp.interface(db) else {
                continue;
            };
            let _ = writeln!(
                out,
                "  {}   ({})",
                iface.qualified_name(db).render_user_facing(),
                location(db, imp.file(db), imp.span(db)),
            );
        }
    }
}

fn render_class(db: &ProjectDatabase, class: Class<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "class {}{}   {}",
        class.qualified_name(db).render_user_facing(),
        render_generics(&class.generic_params(db)),
        location(db, class.file(db), class.name_span(db)),
    );
    push_docstring(&mut out, class.docstring(db));

    let fields = class.fields(db);
    if !fields.is_empty() {
        // Everything is public today (a `_` prefix is convention, not
        // semantics); revisit when the language grows visibility.
        let _ = writeln!(out, "\nfields:");
        for field in &fields {
            let _ = writeln!(out, "  {} {}", field.name(db), ty(field.ty(db)));
            if let Some(first) = field.docstring(db).and_then(|d| d.lines().next()) {
                let _ = writeln!(out, "      {first}");
            }
        }
    }

    // Hide `_`-shims (stdlib-internal by convention), `$`-named companions,
    // and auto-derived methods (`to_json`/`from_json`), matching what the
    // language surface presents.
    let mut hidden = 0usize;
    let visible: Vec<_> = class
        .methods(db)
        .into_iter()
        .filter(|m| {
            let name = m.name(db);
            let internal = name.as_str().starts_with('_')
                || name.as_str().contains('$')
                || matches!(m.origin(db), baml_surface::FunctionOrigin::AutoDerive);
            hidden += usize::from(internal);
            !internal
        })
        .collect();
    let (statics, instance): (Vec<_>, Vec<_>) = visible.into_iter().partition(|m| {
        m.signature(db)
            .params
            .first()
            .is_none_or(|p| p.name.as_ref().map(|n| n.as_str()) != Some("self"))
    });
    push_method_list(&mut out, db, "methods", &instance);
    push_method_list(&mut out, db, "static methods", &statics);
    if hidden > 0 {
        let _ = writeln!(out, "  ({hidden} compiler-synthesized method(s) hidden)");
    }
    push_impls(&mut out, db, &class.trait_impls(db), true);
    out
}

fn render_enum(db: &ProjectDatabase, enm: Enum<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "enum {}   {}",
        enm.qualified_name(db).render_user_facing(),
        location(db, enm.file(db), enm.name_span(db)),
    );
    push_docstring(&mut out, enm.docstring(db));
    let _ = writeln!(out, "\nvariants:");
    for variant in enm.variants(db) {
        let _ = writeln!(out, "  {}", variant.name(db));
        if let Some(first) = variant.docstring(db).and_then(|d| d.lines().next()) {
            let _ = writeln!(out, "      {first}");
        }
    }
    push_impls(&mut out, db, &enm.trait_impls(db), false);
    out
}

fn render_interface(db: &ProjectDatabase, iface: Interface<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "interface {}   {}",
        iface.qualified_name(db).render_user_facing(),
        location(db, iface.file(db), iface.name_span(db)),
    );
    push_docstring(&mut out, iface.docstring(db));

    let assoc = iface.assoc_types(db);
    if !assoc.is_empty() {
        let _ = writeln!(out, "\nassociated types:");
        for a in assoc {
            match a.default_ty(db) {
                Some(default) => {
                    let _ = writeln!(out, "  type {} = {}", a.name(db), ty(&default));
                }
                None => {
                    let _ = writeln!(out, "  type {}", a.name(db));
                }
            }
        }
    }

    let required = iface.required_methods(db);
    if !required.is_empty() {
        let _ = writeln!(out, "\nrequired methods:");
        for method in required {
            let resolved = method.resolved(db);
            let _ = writeln!(out, "  {}", render_required(db, &method, resolved));
            if let Some(first) = method.docstring(db).and_then(|d| d.lines().next()) {
                let _ = writeln!(out, "      {first}");
            }
        }
    }
    push_method_list(&mut out, db, "default methods", &iface.default_methods(db));

    let implementors = iface.implementors(db);
    if !implementors.is_empty() {
        let _ = writeln!(out, "\nimplemented for:");
        for imp in implementors {
            let for_ty = imp.for_ty(db).map(ty).unwrap_or_else(|| "?".to_string());
            let _ = writeln!(
                out,
                "  {for_ty}   ({})",
                location(db, imp.file(db), imp.span(db))
            );
        }
    }
    out
}

fn render_required(
    db: &dyn Db,
    method: &baml_surface::RequiredMethod<'_>,
    resolved: &baml_surface::facts::ResolvedInterfaceMethod,
) -> String {
    let generics: Vec<String> = resolved
        .generic_params
        .iter()
        .map(|(param, bounds)| {
            if bounds.is_empty() {
                param.as_str().to_string()
            } else {
                let names: Vec<String> =
                    bounds.iter().map(|b| b.name.render_user_facing()).collect();
                format!("{} extends {}", param.as_str(), names.join(" & "))
            }
        })
        .collect();
    let generics = if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", generics.join(", "))
    };
    match &resolved.function_ty {
        baml_type::Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let params: Vec<String> = params
                .iter()
                .map(|p| {
                    let name = p.name.as_ref().map_or("_", |n| n.as_str());
                    if name == "self" {
                        "self".to_string()
                    } else {
                        format!("{name}: {}", ty(&p.ty))
                    }
                })
                .collect();
            let mut line = format!(
                "function {}{generics}({}) -> {}",
                method.name(db),
                params.join(", "),
                ty(ret)
            );
            if !matches!(**throws, baml_type::Ty::Never { .. }) {
                let _ = write!(line, " throws {}", ty(throws));
            }
            line
        }
        other => format!("function {}{generics}: {}", method.name(db), ty(other)),
    }
}

fn render_type_alias(db: &ProjectDatabase, alias: TypeAlias<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "type {} = {}   {}",
        alias.qualified_name(db).render_user_facing(),
        ty(&alias.resolved(db)),
        location(db, alias.file(db), alias.name_span(db)),
    );
    push_docstring(&mut out, alias.docstring(db));
    out
}

fn render_function(db: &ProjectDatabase, function: Function<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}   {}",
        render_signature(db, function),
        location(db, function.file(db), function.name_span(db)),
    );
    push_docstring(&mut out, function.docstring(db));
    let throws = function.throws(db);
    if throws.declared.is_none() && !matches!(throws.effective, baml_type::Ty::Never { .. }) {
        let _ = writeln!(out, "\n  (throws inferred from the body)");
    }
    if !throws.panics.is_empty() {
        let panics: Vec<String> = throws.panics.iter().map(ty).collect();
        let _ = writeln!(out, "  panics: {}", panics.join(", "));
    }
    out
}

/// Render a resolved symbol.
pub fn render_symbol(db: &ProjectDatabase, symbol: Symbol<'_>) -> String {
    match symbol {
        Symbol::Class(class) => render_class(db, class),
        Symbol::Enum(enm) => render_enum(db, enm),
        Symbol::Interface(iface) => render_interface(db, iface),
        Symbol::TypeAlias(alias) => render_type_alias(db, alias),
        Symbol::Function(function) => render_function(db, function),
        // Config-ish kinds: a header line plus their few structural facts.
        Symbol::TemplateString(t) => {
            let mut out = String::new();
            let _ = writeln!(
                out,
                "template_string {}   {}",
                t.name(db),
                location(db, t.file(db), t.name_span(db)),
            );
            out
        }
        Symbol::Client(c) => {
            let mut out = String::new();
            let _ = writeln!(
                out,
                "client {}   {}",
                c.name(db),
                location(db, c.file(db), c.name_span(db)),
            );
            out
        }
        Symbol::Test(t) => {
            let mut out = String::new();
            let _ = writeln!(
                out,
                "test {}   {}",
                t.name(db),
                location(db, t.file(db), t.name_span(db)),
            );
            let refs = t.function_refs(db);
            if !refs.is_empty() {
                let names: Vec<String> = refs.iter().map(ToString::to_string).collect();
                let _ = writeln!(out, "  functions: {}", names.join(", "));
            }
            out
        }
        Symbol::RetryPolicy(r) => {
            let mut out = String::new();
            let _ = writeln!(
                out,
                "retry_policy {}   {}",
                r.name(db),
                location(db, r.file(db), r.name_span(db)),
            );
            out
        }
        Symbol::Global(g) => {
            let kind = match g.origin(db) {
                baml_surface::LetOrigin::Client => "client",
                baml_surface::LetOrigin::RetryPolicy => "retry_policy",
                baml_surface::LetOrigin::Source => "let",
            };
            let mut out = String::new();
            let _ = writeln!(
                out,
                "{kind} {}   {}",
                g.name(db),
                location(db, g.file(db), g.name_span(db)),
            );
            out
        }
        Symbol::Impl(imp) => {
            let mut out = String::new();
            // Rendering an impl directly describes the declaration itself,
            // rather than claiming applicability for a particular type.
            push_impls(&mut out, db, &[imp], true);
            out
        }
    }
}

/// Render a member drill-in.
pub fn render_member(db: &ProjectDatabase, owner: Symbol<'_>, member: Member<'_>) -> String {
    let mut out = String::new();
    let owner_name = owner
        .name(db)
        .map_or_else(|| "?".to_string(), |n| n.to_string());
    match member {
        Member::Method(function) => {
            let _ = writeln!(
                out,
                "{}   {}",
                render_signature(db, function),
                location(db, function.file(db), function.name_span(db)),
            );
            push_docstring(&mut out, function.docstring(db));
        }
        Member::RequiredMethod(method) => {
            let resolved = method.resolved(db);
            let _ = writeln!(
                out,
                "{}   (required by {owner_name})",
                render_required(db, &method, resolved)
            );
            push_docstring(&mut out, method.docstring(db));
        }
        Member::Field(field) => {
            let _ = writeln!(out, "{owner_name}.{} {}", field.name(db), ty(field.ty(db)));
            push_docstring(&mut out, field.docstring(db));
        }
        Member::Variant(variant) => {
            let _ = writeln!(out, "{owner_name}.{}", variant.name(db));
            push_docstring(&mut out, variant.docstring(db));
        }
        Member::AssocType(assoc) => match assoc.default_ty(db) {
            Some(default) => {
                let _ = writeln!(
                    out,
                    "type {owner_name}.{} = {}",
                    assoc.name(db),
                    ty(&default)
                );
            }
            None => {
                let _ = writeln!(out, "type {owner_name}.{}", assoc.name(db));
            }
        },
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use baml_project::ProjectDatabase;

    use super::{render_member, render_symbol};

    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));
        db
    }

    fn render(db: &ProjectDatabase, path: &str) -> String {
        match baml_surface::resolve(db, path) {
            Some(baml_surface::Resolved::Symbol(symbol)) => render_symbol(db, symbol),
            Some(baml_surface::Resolved::Member(owner, member)) => render_member(db, owner, member),
            other => panic!("{path} resolves to a renderable target, got {other:?}"),
        }
    }

    #[test]
    fn renders_builtin_interface() {
        let db = make_db();
        insta::assert_snapshot!(render(&db, "baml.Sortable"));
    }

    #[test]
    fn renders_builtin_class_with_impls() {
        let db = make_db();
        insta::assert_snapshot!(render(&db, "baml.time.Duration"));
        // Every kind view is an ordinary wrapper class, so all nine render
        // the `reflect.AnyClass` blanket row.
        assert!(render(&db, "reflect.class.Type").contains("reflect.AnyClass"));
        assert!(render(&db, "reflect.enum.Type").contains("reflect.AnyClass"));
    }

    #[test]
    fn renders_member_drill() {
        let db = make_db();
        insta::assert_snapshot!(render(&db, "baml.Comparable.compare"));
    }

    #[test]
    fn renders_user_items() {
        let mut db = make_db();
        db.add_file(
            "app.baml",
            r#"/// Widget docs.
class Widget {
  /// Visible name.
  name string
  _cache string
}

enum Mode { Fast, Slow }

type Id = string

/// Greets.
function greet(name: string) -> string throws baml.errors.Io {
  name
}
"#,
        );
        let mut out = String::new();
        for path in ["Widget", "Mode", "Id", "greet", "Widget.name"] {
            let _ = writeln!(out, "════ {path} ════");
            out.push_str(&render(&db, path));
        }
        insta::assert_snapshot!(out);
    }
}
