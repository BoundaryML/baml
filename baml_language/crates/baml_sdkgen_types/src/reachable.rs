//! Prune built-in types that no generated declaration can reach.

use std::collections::HashSet;

use crate::{Function, Name, Symbol, SymbolPool, Ty};

/// Keep every function and user declaration, plus only the built-in types
/// reachable from them or from the always-kept `baml.media`, `baml.errors`,
/// `baml.panics`, `ai.errors`, and `ai.stream` namespaces. Reachability
/// follows class properties and methods, function signatures (arguments,
/// return, throws, watchers), and alias targets; a type token (`Ty::Type`)
/// reaches `reflect.Type`.
pub fn without_unreachable_builtin_types(pool: &SymbolPool) -> SymbolPool {
    // `Ty::Type` is a type token, which SDKs render as the bridge-owned
    // `reflect.Type` class.
    let reflect_type = Name::new(
        baml_base::Name::new("reflect"),
        Vec::new(),
        baml_base::Name::new("Type"),
    );
    let mut reachable: HashSet<&Name> = HashSet::new();
    let mut pending: Vec<&Name> = pool
        .iter()
        .filter(|(name, symbol)| is_root(name, symbol))
        .map(|(name, _)| name)
        .collect();
    while let Some(name) = pending.pop() {
        if !reachable.insert(name) {
            continue;
        }
        let Some(symbol) = pool.get(name) else {
            continue;
        };
        for_each_type(symbol, &mut |ty| {
            crate::visit_type(ty, &mut |ty| {
                let referenced = match ty {
                    Ty::Type => Some(&reflect_type),
                    _ => named_reference(ty),
                };
                if let Some(referenced) = referenced
                    && let Some((referenced, _)) = pool.get_key_value(referenced)
                    && !reachable.contains(referenced)
                {
                    pending.push(referenced);
                }
            });
        });
    }
    pool.iter()
        .filter(|(name, _)| reachable.contains(name))
        .map(|(name, symbol)| (name.clone(), symbol.clone()))
        .collect()
}

/// Built-in namespaces whose types are always generated, as
/// `(package, namespace)`. Signatures cannot reach everything the host
/// sees: the media types are bridge-owned classes, and the runtime hands the
/// host error, panic, and stream-event values that no signature names.
const ALWAYS_KEPT_NAMESPACES: &[(&str, &str)] = &[
    ("baml", "media"),
    ("baml", "errors"),
    ("baml", "panics"),
    ("ai", "errors"),
    ("ai", "stream"),
];

fn is_root(name: &Name, symbol: &Symbol) -> bool {
    let origin = match symbol {
        Symbol::Function(_) => return true,
        Symbol::Class(class) => &class.origin,
        Symbol::Enum(enumeration) => &enumeration.origin,
        Symbol::TypeAlias(alias) => &alias.origin,
    };
    !origin.source_file_path.starts_with("<builtin>/") || is_always_kept(name)
}

fn is_always_kept(name: &Name) -> bool {
    ALWAYS_KEPT_NAMESPACES.iter().any(|(package, namespace)| {
        name.package().as_str() == *package
            && name
                .namespace()
                .iter()
                .map(baml_base::Name::as_str)
                .eq([*namespace])
    })
}

/// The pool symbol a type node names directly, if any.
fn named_reference(ty: &Ty) -> Option<&Name> {
    match ty {
        Ty::Class(name, _) | Ty::Enum(name) | Ty::EnumVariant(name, _) | Ty::TypeAlias(name) => {
            Some(name)
        }
        _ => None,
    }
}

/// Call `f` on every type a symbol's generated declaration mentions.
fn for_each_type<'a>(symbol: &'a Symbol, f: &mut impl FnMut(&'a Ty)) {
    fn function<'a>(function: &'a Function, f: &mut impl FnMut(&'a Ty)) {
        for argument in &function.arguments {
            f(&argument.ty);
        }
        f(&function.return_type);
        if let Some(throws) = &function.throws {
            f(throws);
        }
        for (_, watcher) in &function.watchers {
            f(watcher);
        }
    }
    match symbol {
        Symbol::Function(value) => function(value, f),
        Symbol::Class(class) => {
            for property in &class.properties {
                f(&property.ty);
            }
            for method in class.static_methods.iter().chain(&class.instance_methods) {
                function(method, f);
            }
        }
        Symbol::TypeAlias(alias) => f(&alias.resolves_to),
        Symbol::Enum(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

    use super::*;
    use crate::{Class, ClassProperty, Enum, FunctionArgument, Origin, TypeAlias};

    const BUILTIN: &str = "<builtin>/baml/sample.baml";
    const USER: &str = "main.baml";

    fn name(package: &str, namespace: &[&str], value: &str) -> Name {
        Name::new(
            BaseName::new(package),
            namespace
                .iter()
                .map(|segment| BaseName::new(*segment))
                .collect(),
            BaseName::new(value),
        )
    }

    fn origin(source_file_path: &str) -> Origin {
        Origin {
            source_file_path: source_file_path.to_string(),
            span_start: 0,
        }
    }

    fn class(name: &Name, source: &str, fields: Vec<Ty>) -> Symbol {
        Symbol::Class(Class {
            name: name.clone(),
            generic_params: Vec::new(),
            docstring: None,
            properties: fields
                .into_iter()
                .enumerate()
                .map(|(index, ty)| ClassProperty {
                    name: BaseName::new(format!("field{index}")),
                    docstring: None,
                    ty,
                })
                .collect(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: origin(source),
        })
    }

    fn enumeration(name: &Name, source: &str) -> Symbol {
        Symbol::Enum(Enum {
            name: name.clone(),
            docstring: None,
            variants: Vec::new(),
            origin: origin(source),
        })
    }

    fn alias(name: &Name, source: &str, resolves_to: Ty) -> Symbol {
        Symbol::TypeAlias(TypeAlias {
            name: name.clone(),
            resolves_to,
            recursive: false,
            origin: origin(source),
        })
    }

    fn function(source: &str, argument: Ty, return_type: Ty) -> Symbol {
        Symbol::Function(Function {
            name: BaseName::new("f"),
            generic_params: Vec::new(),
            docstring: None,
            arguments: vec![FunctionArgument {
                name: BaseName::new("x"),
                docstring: None,
                ty: argument,
                default: None,
                injected: false,
            }],
            return_type,
            throws: None,
            watchers: Vec::new(),
            origin: origin(source),
        })
    }

    fn class_ty(name: &Name) -> Ty {
        Ty::Class(name.clone(), Box::new([]))
    }

    fn kept(pool: &SymbolPool) -> Vec<String> {
        let mut names: Vec<String> = without_unreachable_builtin_types(pool)
            .keys()
            .map(ToString::to_string)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn keeps_builtin_types_reachable_from_user_declarations_and_always_kept_namespaces() {
        let user_class = name("user", &[], "Resume");
        let user_function = name("user", &[], "extract");
        let via_field = name("baml", &["http"], "Response");
        let via_alias = name("baml", &["http"], "Headers");
        let alias_target = name("baml", &["http"], "Header");
        let via_signature = name("baml", &["errors"], "Kind");
        let generic_arg = name("baml", &["ai"], "Message");
        let cycle_a = name("baml", &["graph"], "A");
        let cycle_b = name("baml", &["graph"], "B");
        let image = name("baml", &["media"], "Image");
        let io_error = name("baml", &["errors"], "Io");
        let cancelled = name("baml", &["panics"], "Cancelled");
        let llm_error = name("ai", &["errors"], "ClientError");
        let stream_done = name("ai", &["stream"], "Done");
        let stack_trace = name("baml", &["errors"], "StackTrace");
        let frame = name("baml", &["trace"], "Frame");
        let unreachable = name("baml", &["fs"], "File");
        let from_unreachable = name("baml", &["fs"], "Mode");

        let pool = SymbolPool::from([
            (
                user_class.clone(),
                class(
                    &user_class,
                    USER,
                    vec![
                        Ty::List(Box::new(class_ty(&via_field))),
                        Ty::Class(cycle_a.clone(), Box::new([class_ty(&generic_arg)])),
                    ],
                ),
            ),
            (
                user_function,
                function(
                    USER,
                    Ty::EnumVariant(via_signature.clone(), BaseName::new("Timeout")),
                    Ty::TypeAlias(via_alias.clone()),
                ),
            ),
            (via_field.clone(), class(&via_field, BUILTIN, Vec::new())),
            (
                via_alias.clone(),
                alias(
                    &via_alias,
                    BUILTIN,
                    Ty::Map {
                        key: Box::new(Ty::String),
                        value: Box::new(class_ty(&alias_target)),
                    },
                ),
            ),
            (
                alias_target.clone(),
                class(&alias_target, BUILTIN, Vec::new()),
            ),
            (via_signature.clone(), enumeration(&via_signature, BUILTIN)),
            (
                generic_arg.clone(),
                class(&generic_arg, BUILTIN, Vec::new()),
            ),
            (
                cycle_a.clone(),
                class(&cycle_a, BUILTIN, vec![class_ty(&cycle_b)]),
            ),
            (
                cycle_b.clone(),
                class(&cycle_b, BUILTIN, vec![class_ty(&cycle_a)]),
            ),
            (image.clone(), class(&image, BUILTIN, Vec::new())),
            (
                io_error.clone(),
                class(&io_error, BUILTIN, vec![class_ty(&stack_trace)]),
            ),
            (
                stack_trace.clone(),
                class(&stack_trace, BUILTIN, vec![class_ty(&frame)]),
            ),
            (frame.clone(), class(&frame, BUILTIN, Vec::new())),
            (cancelled.clone(), class(&cancelled, BUILTIN, Vec::new())),
            (llm_error.clone(), class(&llm_error, BUILTIN, Vec::new())),
            (
                stream_done.clone(),
                class(&stream_done, BUILTIN, Vec::new()),
            ),
            (
                unreachable.clone(),
                class(
                    &unreachable,
                    BUILTIN,
                    vec![Ty::Enum(from_unreachable.clone())],
                ),
            ),
            (
                from_unreachable.clone(),
                enumeration(&from_unreachable, BUILTIN),
            ),
        ]);

        assert_eq!(
            kept(&pool),
            [
                "ai.errors.ClientError",
                "ai.stream.Done",
                "baml.ai.Message",
                "baml.errors.Io",
                "baml.errors.Kind",
                "baml.errors.StackTrace",
                "baml.graph.A",
                "baml.graph.B",
                "baml.http.Header",
                "baml.http.Headers",
                "baml.http.Response",
                "baml.media.Image",
                "baml.panics.Cancelled",
                "baml.trace.Frame",
                "user.Resume",
                "user.extract",
            ]
        );
    }

    #[test]
    fn type_tokens_reach_reflect_type() {
        let reflect_type = name("reflect", &[], "Type");
        let unused = name("reflect", &[], "Signature");
        let pool = SymbolPool::from([
            (name("user", &[], "f"), function(USER, Ty::Type, Ty::Int)),
            (
                reflect_type.clone(),
                class(&reflect_type, BUILTIN, Vec::new()),
            ),
            (unused.clone(), class(&unused, BUILTIN, Vec::new())),
        ]);
        assert_eq!(kept(&pool), ["reflect.Type", "user.f"]);
    }

    #[test]
    fn builtin_types_reachable_only_from_kept_builtin_functions_are_kept() {
        let builtin_function = name("baml", &["fs"], "open");
        let file = name("baml", &["fs"], "File");
        let pool = SymbolPool::from([
            (
                builtin_function,
                function(BUILTIN, Ty::String, class_ty(&file)),
            ),
            (file.clone(), class(&file, BUILTIN, Vec::new())),
        ]);
        assert_eq!(kept(&pool), ["baml.fs.File", "baml.fs.open"]);
    }
}
