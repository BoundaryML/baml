//! Prune built-in types that no generated declaration can reach.

use std::collections::HashSet;

use baml_base::MediaKind;

use crate::{Function, Name, Symbol, SymbolPool, Ty};

/// Keep every function and user declaration, plus only the built-in types
/// reachable from them or from the always-kept `baml.media`, `baml.errors`,
/// `baml.panics`, `ai.errors`, and `ai.stream` namespaces. Reachability
/// follows class properties and methods, function signatures (arguments,
/// return, throws, watchers), and alias targets, through every generic
/// argument and structural child; type tokens (`Ty::Type`) and media reach
/// the `reflect.Type` and `baml.media` classes that SDKs render for them.
pub fn without_unreachable_builtin_types(pool: &SymbolPool) -> SymbolPool {
    let implicit = ImplicitNames::new();
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
        // `visit_type` walks every child, so generic arguments (reified type
        // variables such as `Box<baml.http.Response>`), list/map elements,
        // union members, and callable/future parts are all followed.
        for_each_type(symbol, &mut |ty| {
            crate::visit_type(ty, &mut |ty| {
                for_each_named(ty, &implicit, &mut |referenced| {
                    if let Some((referenced, _)) = pool.get_key_value(referenced)
                        && !reachable.contains(referenced)
                    {
                        pending.push(referenced);
                    }
                });
            });
        });
    }
    pool.iter()
        .filter(|(name, _)| reachable.contains(name))
        .map(|(name, symbol)| (name.clone(), symbol.clone()))
        .collect()
}

/// Pool symbols that SDKs render for types which do not carry a name.
struct ImplicitNames {
    reflect_type: Name,
    media: [Name; 4],
}

impl ImplicitNames {
    fn new() -> Self {
        let name = |package: &str, namespace: &[&str], item: &str| {
            Name::new(
                baml_base::Name::new(package),
                namespace
                    .iter()
                    .map(|segment| baml_base::Name::new(*segment))
                    .collect(),
                baml_base::Name::new(item),
            )
        };
        Self {
            reflect_type: name("reflect", &[], "Type"),
            media: [
                name("baml", &["media"], "Image"),
                name("baml", &["media"], "Audio"),
                name("baml", &["media"], "Video"),
                name("baml", &["media"], "Pdf"),
            ],
        }
    }

    fn media(&self, kind: MediaKind) -> &[Name] {
        match kind {
            MediaKind::Image => &self.media[0..1],
            MediaKind::Audio => &self.media[1..2],
            MediaKind::Video => &self.media[2..3],
            MediaKind::Pdf => &self.media[3..4],
            MediaKind::Generic => &self.media,
        }
    }
}

/// Call `reach` for each pool symbol that one type node names itself; its
/// children are visited separately. The match is exhaustive so a new type
/// variant needs an explicit reachability decision.
fn for_each_named<'a>(ty: &'a Ty, implicit: &'a ImplicitNames, reach: &mut impl FnMut(&'a Name)) {
    match ty {
        Ty::Class(name, _) | Ty::Enum(name) | Ty::EnumVariant(name, _) | Ty::TypeAlias(name) => {
            reach(name);
        }
        // SDKs render a type token as the bridge-owned `reflect.Type` class.
        Ty::Type => reach(&implicit.reflect_type),
        // SDKs render media as the bridge-owned `baml.media` classes.
        Ty::Media(kind) => implicit.media(*kind).iter().for_each(reach),
        // Interfaces are emitted as tokens, not pool symbols; their type
        // arguments and associated types are children.
        Ty::Interface(..) => {}
        // Structural types name nothing themselves; their parts are children.
        Ty::List(_) | Ty::Map { .. } | Ty::Union(_) | Ty::Function { .. } | Ty::Future(..) => {}
        // Leaves that name no declaration.
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Uint8Array
        | Ty::Literal(..)
        | Ty::TypeVar(..)
        | Ty::RustType
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Void
        | Ty::Unknown
        | Ty::Never => {}
    }
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
    fn function_signatures_reach_through_generic_arguments_and_structure() {
        let n = |item: &str| name("baml", &["probe"], item);
        let [
            in_user_generic,
            builtin_generic,
            in_builtin_generic,
            from_builtin_generic_field,
            map_value,
            callable_param,
            callable_return,
            future_value,
            future_error,
            interface_arg,
            thrown,
            unreferenced,
        ] = [
            "InUserGeneric",
            "BuiltinGeneric",
            "InBuiltinGeneric",
            "FromBuiltinGenericField",
            "MapValue",
            "CallableParam",
            "CallableReturn",
            "FutureValue",
            "FutureError",
            "InterfaceArg",
            "Thrown",
            "Unreferenced",
        ]
        .map(n);
        let holder = name("user", &[], "Holder");
        let generic_function = name("user", &[], "g");

        // Argument: a user generic reified with a stdlib type, inside a list.
        let argument = Ty::List(Box::new(Ty::Class(
            holder.clone(),
            Box::new([class_ty(&in_user_generic)]),
        )));
        // Return: a stdlib generic reified with a stdlib type, inside a map
        // value, alongside a callable, a future, and an interface argument.
        let return_type = Ty::Union(Box::new([
            Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::Class(
                    builtin_generic.clone(),
                    Box::new([class_ty(&in_builtin_generic)]),
                )),
            },
            class_ty(&map_value),
            Ty::Function {
                params: Box::new([crate::CallableParam::required(
                    None,
                    class_ty(&callable_param),
                )]),
                ret: Box::new(class_ty(&callable_return)),
                throws: Box::new(Ty::Never),
            },
            Ty::Future(
                Box::new(class_ty(&future_value)),
                Box::new(class_ty(&future_error)),
            ),
            Ty::Interface(
                name("baml", &["probe"], "Iterable"),
                Box::new([class_ty(&interface_arg)]),
                Box::new([]),
            ),
        ]));
        let Symbol::Function(mut signature) = function(USER, argument, return_type) else {
            unreachable!();
        };
        signature.throws = Some(class_ty(&thrown));

        let mut pool = SymbolPool::from([
            (generic_function, Symbol::Function(signature)),
            (
                holder.clone(),
                class(
                    &holder,
                    USER,
                    vec![Ty::TypeVar(crate::ParamTy::new(0, "T".into()))],
                ),
            ),
            (
                builtin_generic.clone(),
                class(
                    &builtin_generic,
                    BUILTIN,
                    vec![class_ty(&from_builtin_generic_field)],
                ),
            ),
        ]);
        for symbol in [
            &in_user_generic,
            &in_builtin_generic,
            &from_builtin_generic_field,
            &map_value,
            &callable_param,
            &callable_return,
            &future_value,
            &future_error,
            &interface_arg,
            &thrown,
            &unreferenced,
        ] {
            pool.insert(symbol.clone(), class(symbol, BUILTIN, Vec::new()));
        }

        assert_eq!(
            kept(&pool),
            [
                "baml.probe.BuiltinGeneric",
                "baml.probe.CallableParam",
                "baml.probe.CallableReturn",
                "baml.probe.FromBuiltinGenericField",
                "baml.probe.FutureError",
                "baml.probe.FutureValue",
                "baml.probe.InBuiltinGeneric",
                "baml.probe.InUserGeneric",
                "baml.probe.InterfaceArg",
                "baml.probe.MapValue",
                "baml.probe.Thrown",
                "user.Holder",
                "user.g",
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
