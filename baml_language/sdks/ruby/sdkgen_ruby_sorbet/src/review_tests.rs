use super::*;
use baml_sdkgen_types::{Enum, EnumVariant, Function, Origin, Ty};

fn alias_diamond_pool(depth: u32, leaf: &Ty) -> SymbolPool {
    use baml_sdkgen_types::TypeAlias;
    let mut pool = SymbolPool::new();
    for index in 0..=depth {
        let name = Name::new("user".into(), vec![], format!("A{index}").into());
        let resolves_to = if index == 0 {
            leaf.clone()
        } else {
            let previous = Ty::TypeAlias(Name::new(
                "user".into(),
                vec![],
                format!("A{}", index - 1).into(),
            ));
            Ty::Union(Box::new([
                Ty::List(Box::new(previous.clone())),
                Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(previous),
                },
            ]))
        };
        pool.insert(
            name.clone(),
            Symbol::TypeAlias(TypeAlias {
                name,
                resolves_to,
                recursive: false,
                origin: Origin {
                    source_file_path: "test.baml".into(),
                    span_start: index,
                },
            }),
        );
    }
    pool
}

#[test]
fn alias_diamond_chain_stays_small_and_fast() {
    let mut pool = alias_diamond_pool(18, &Ty::Int);
    let started = std::time::Instant::now();
    let (files, skipped) =
        to_source_code_with_bytecode_and_skipped(&pool, &[], NamingConvention::Language, None);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(skipped.is_empty());
    let size: usize = files.values().map(String::len).sum();
    assert!(size < 32_000, "generated {size} bytes");
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(root.contains("BamlSdk::A18 = ::T.type_alias { ::T.any(::T::Array[::BamlSdk::A17], ::T::Hash[::String, ::BamlSdk::A17]) }"), "{root}");
    assert!(!root.contains("Input ="));
    // A refused leaf must remove every alias that depends on it.
    if let Some(Symbol::TypeAlias(alias)) =
        pool.get_mut(&Name::new("user".into(), vec![], "A0".into()))
    {
        alias.resolves_to = Ty::Literal(
            baml_base::Literal::Float("1.5".into()),
            baml_type::Freshness::Regular,
        );
    }
    let (_, skipped) =
        to_source_code_with_bytecode_and_skipped(&pool, &[], NamingConvention::Language, None);
    assert_eq!(skipped.len(), 19);
}

#[test]
fn function_signatures_over_alias_diamonds_stay_fast_and_detect_callables() {
    // Bound the entire query, not just an elapsed-time assertion after an
    // exponential walk. A regressed worker must not stall the test process.
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for callable in [false, true] {
            for argument in [false, true] {
                let leaf = if callable {
                    Ty::Function {
                        params: Box::new([]),
                        ret: Box::new(Ty::Int),
                        throws: Box::new(Ty::Never),
                    }
                } else {
                    Ty::Int
                };
                let mut pool = alias_diamond_pool(26, &leaf);
                let alias = Ty::TypeAlias(Name::new("user".into(), vec![], "A26".into()));
                pool.insert(
                    Name::new("user".into(), vec![], "probe".into()),
                    Symbol::Function(Function {
                        name: "probe".into(),
                        generic_params: vec![],
                        docstring: None,
                        arguments: if argument {
                            vec![baml_sdkgen_types::FunctionArgument {
                                name: "value".into(),
                                docstring: None,
                                ty: alias.clone(),
                                default: None,
                                injected: false,
                            }]
                        } else {
                            vec![]
                        },
                        return_type: if argument { Ty::Int } else { alias },
                        throws: None,
                        watchers: vec![],
                        origin: Origin {
                            source_file_path: "test.baml".into(),
                            span_start: 27,
                        },
                    }),
                );
                let (files, skipped) = to_source_code_with_bytecode_and_skipped(
                    &pool,
                    &[],
                    NamingConvention::Language,
                    None,
                );
                assert!(skipped.is_empty());
                let source = &files[&PathBuf::from("baml_sdk/__root.rb")];
                assert!(source.contains("def self.probe("));
                assert_eq!(
                    source.matches(".checked(:never)").count(),
                    if callable { 2 } else { 0 },
                    "callable={callable}, argument={argument}: {source}"
                );
                let size: usize = files.values().map(String::len).sum();
                assert!(size < 48_000, "generated {size} bytes");
            }
        }
        send.send(()).unwrap();
    });
    receive
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("26-level alias diamond signatures must generate within one second");
}

fn recursive_alias_chain(depth: u32, cycle_to_top: bool) -> HashMap<PathBuf, String> {
    use baml_sdkgen_types::TypeAlias;
    let name = |index| Name::new("user".into(), vec![], format!("A{index}").into());
    let mut pool = SymbolPool::new();
    for index in 0..=depth {
        let resolves_to = if index == 0 {
            Ty::Union(Box::new([
                Ty::Int,
                Ty::List(Box::new(Ty::TypeAlias(name(if cycle_to_top {
                    depth
                } else {
                    0
                })))),
            ]))
        } else {
            let previous = Ty::TypeAlias(name(index - 1));
            Ty::Union(Box::new([
                Ty::List(Box::new(previous.clone())),
                Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(previous),
                },
            ]))
        };
        pool.insert(
            name(index),
            Symbol::TypeAlias(TypeAlias {
                name: name(index),
                resolves_to,
                recursive: true,
                origin: Origin {
                    source_file_path: "probe.baml".into(),
                    span_start: index,
                },
            }),
        );
    }
    let started = std::time::Instant::now();
    let (files, skipped) =
        to_source_code_with_bytecode_and_skipped(&pool, &[], NamingConvention::Language, None);
    let elapsed = started.elapsed();
    let size: usize = files.values().map(String::len).sum();
    assert!(elapsed < std::time::Duration::from_secs(5));
    assert!(size < 32_000, "generated {size} bytes");
    assert!(skipped.is_empty());
    files
}

#[test]
fn chain_reaching_a_recursive_alias_stays_small_and_fast() {
    let files = recursive_alias_chain(16, false);
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(
        root.contains(
            "BamlSdk::A0 = ::T.type_alias { ::T.any(::Integer, ::T::Array[::T.untyped]) }"
        ),
        "{root}"
    );
    assert!(root.contains("BamlSdk::A16 = ::T.type_alias { ::T.any(::T::Array[::BamlSdk::A15], ::T::Hash[::String, ::BamlSdk::A15]) }"), "{root}");
}

#[test]
fn chain_in_one_recursive_component_stays_small_and_fast() {
    let files = recursive_alias_chain(14, true);
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(
        root.contains(
            "BamlSdk::A0 = ::T.type_alias { ::T.any(::Integer, ::T::Array[::T.untyped]) }"
        ),
        "{root}"
    );
    assert!(root.contains("BamlSdk::A14 = ::T.type_alias { ::T.any(::T::Array[::T.untyped], ::T::Hash[::String, ::T.untyped]) }"), "{root}");
}

#[test]
fn recursive_aliases_outside_the_owner_component_keep_input_companions() {
    use baml_sdkgen_types::TypeAlias;
    let rec = Name::new("user".into(), vec![], "Rec".into());
    let wrapper = Name::new("user".into(), vec![], "Wrapper".into());
    let pool = SymbolPool::from([
        (
            rec.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: rec.clone(),
                resolves_to: Ty::Union(Box::new([
                    Ty::Float,
                    Ty::List(Box::new(Ty::TypeAlias(rec.clone()))),
                ])),
                recursive: true,
                origin: Origin {
                    source_file_path: "test.baml".into(),
                    span_start: 0,
                },
            }),
        ),
        (
            wrapper.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: wrapper,
                resolves_to: Ty::List(Box::new(Ty::TypeAlias(rec))),
                recursive: true,
                origin: Origin {
                    source_file_path: "test.baml".into(),
                    span_start: 1,
                },
            }),
        ),
    ]);
    let files = to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(
        root.contains("BamlSdk::Wrapper = ::T.type_alias { ::T::Array[::BamlSdk::Rec] }"),
        "{root}"
    );
    assert!(
        root.contains("BamlSdk::WrapperInput = ::T.type_alias { ::T::Array[::BamlSdk::RecInput] }"),
        "{root}"
    );
    assert!(root.contains("BamlSdk::RecInput = ::T.type_alias { ::T.any(::T.any(::Float, ::Integer), ::T::Array[::T.untyped]) }"), "{root}");
}

#[test]
fn input_aliases_preserve_transitive_float_widening_and_escape_user_constants() {
    use baml_sdkgen_types::TypeAlias;
    let mut pool = SymbolPool::new();
    for (leaf, resolves_to) in [
        ("Floats", Ty::List(Box::new(Ty::Float))),
        (
            "Nested",
            Ty::List(Box::new(Ty::TypeAlias(Name::new(
                "user".into(),
                vec![],
                "Floats".into(),
            )))),
        ),
        ("FloatsInput", Ty::Int),
    ] {
        let name = Name::new("user".into(), vec![], leaf.into());
        pool.insert(
            name.clone(),
            Symbol::TypeAlias(TypeAlias {
                name,
                resolves_to,
                recursive: false,
                origin: Origin {
                    source_file_path: "test.baml".into(),
                    span_start: 0,
                },
            }),
        );
    }
    let names = NameTable::build(&pool, NamingConvention::Language);
    let supported = pool.keys().cloned().collect();
    let ty = Ty::TypeAlias(Name::new("user".into(), vec![], "Nested".into()));
    assert_eq!(
        types::ruby_input_type(&ty, &supported, &names, &pool, &[]).as_deref(),
        Some("::BamlSdk::NestedInput")
    );
    let files = to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(
        root.contains(
            "BamlSdk::FloatsInput_ = ::T.type_alias { ::T::Array[::T.any(::Float, ::Integer)] }"
        ),
        "{root}"
    );
    assert!(
        root.contains(
            "BamlSdk::NestedInput = ::T.type_alias { ::T::Array[::BamlSdk::FloatsInput_] }"
        ),
        "{root}"
    );
    assert!(
        root.contains("BamlSdk::Nested = ::T.type_alias { ::T::Array[::BamlSdk::Floats] }"),
        "{root}"
    );
}

#[test]
fn program_registration_precedes_declarations_and_namespace_requires() {
    let name = Name::new("user".into(), vec![], "Choice".into());
    let pool = SymbolPool::from([(
        name.clone(),
        Symbol::Enum(Enum {
            name,
            docstring: None,
            variants: vec![],
            origin: Origin {
                source_file_path: "test.baml".into(),
                span_start: 0,
            },
        }),
    )]);
    let files = to_source_code_with_bytecode(&pool, &[1], NamingConvention::Language, None);
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    let registration = root.find("::Baml::Bridge.register_program(").unwrap();
    assert!(
        registration < root.find("module BamlSdk\n").unwrap(),
        "{root}"
    );
    assert!(registration < root.find("Choice").unwrap(), "{root}");
    assert!(
        registration < root.find("require_relative \"baml_sdk/__root\"").unwrap(),
        "{root}"
    );
}

#[test]
fn prompt_return_is_skipped_with_a_warning() {
    use baml_sdkgen_types::{Class, ClassCrossing};
    let prompt = Name::new("ai".into(), vec![], "Prompt".into());
    let origin = Origin {
        source_file_path: "test.baml".into(),
        span_start: 0,
    };
    let function = Name::new("user".into(), vec![], "get_prompt".into());
    let pool = SymbolPool::from([
        (
            prompt.clone(),
            Symbol::Class(Class {
                name: prompt.clone(),
                crossing: ClassCrossing::Prompt,
                generic_params: vec![],
                docstring: None,
                properties: vec![],
                static_methods: vec![],
                instance_methods: vec![],
                origin: origin.clone(),
            }),
        ),
        (
            function,
            Symbol::Function(Function {
                name: "get_prompt".into(),
                generic_params: vec![],
                docstring: None,
                arguments: vec![],
                return_type: Ty::Class(prompt, Box::new([])),
                throws: None,
                watchers: vec![],
                origin,
            }),
        ),
    ]);
    let (files, warnings) =
        to_source_code_with_bytecode_and_warnings(&pool, &[], NamingConvention::Language, None);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.fqn == "user.get_prompt" && warning.local)
    );
    assert!(
        files
            .values()
            .all(|source| !source.contains("def self.get_prompt"))
    );
}

#[test]
fn skips_report_the_users_package_not_a_hardcoded_user_prefix() {
    let mut pool = SymbolPool::new();
    // Even a project named my_app has local names spelled user in the pool.
    for package in ["user", "dependency"] {
        let name = Name::new(package.into(), vec![], "unsupported".into());
        pool.insert(
            name,
            Symbol::Function(Function {
                name: "unsupported".into(),
                generic_params: vec![],
                docstring: None,
                arguments: vec![],
                return_type: Ty::Literal(
                    baml_base::Literal::Float("1.5".into()),
                    baml_type::Freshness::Regular,
                ),
                throws: None,
                watchers: vec![],
                origin: Origin {
                    source_file_path: "test.baml".into(),
                    span_start: 0,
                },
            }),
        );
    }
    let (files, warnings) = to_source_code_with_bytecode_and_warnings(
        &pool,
        &[],
        NamingConvention::Language,
        Some("[package]\nname = \"my_app\"\n"),
    );
    assert_eq!(warnings.len(), 2);
    for warning in warnings {
        assert_eq!(warning.local, warning.fqn.starts_with("user."));
        assert!(!warning.reason.is_empty());
    }
    assert!(
        files
            .values()
            .all(|source| !source.contains("def self.unsupported"))
    );
}

#[test]
fn enum_variants_keep_underscores_and_escape_collisions() {
    let name = Name::new("user".into(), vec![], "Choice".into());
    let pool = SymbolPool::from([(
        name.clone(),
        Symbol::Enum(Enum {
            name,
            docstring: None,
            variants: [
                "UPDATE_QUANTITY",
                "update_quantity",
                "_private",
                "BamlPrivate",
                "_update_quantity",
                "BamlUpdate_quantity",
            ]
            .into_iter()
            .map(|raw| EnumVariant {
                name: raw.into(),
                value: raw.into(),
                docstring: None,
            })
            .collect(),
            origin: Origin {
                source_file_path: "test.baml".into(),
                span_start: 0,
            },
        }),
    )]);
    let files = to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
    let source = &files[&PathBuf::from("baml_sdk/__root.rb")];
    for expected in [
        "UPDATE_QUANTITY = new(\"UPDATE_QUANTITY\")",
        "Update_quantity = new(\"update_quantity\")",
        "BamlPrivate = new(\"BamlPrivate\")",
        "BamlPrivate_ = new(\"_private\")",
        "BamlUpdate_quantity = new(\"BamlUpdate_quantity\")",
        "BamlUpdate_quantity_ = new(\"_update_quantity\")",
    ] {
        assert!(source.contains(expected), "{source}");
    }
}

#[test]
fn package_constants_are_reserved_and_vendor_is_escaped_at_every_depth() {
    let mut pool = SymbolPool::new();
    for (namespace, leaf) in [
        (vec![], "BYTECODE"),
        (vec![], "METADATA"),
        (vec!["nested", "vendor"], "Choice"),
    ] {
        let name = Name::new(
            "user".into(),
            namespace.into_iter().map(Into::into).collect(),
            leaf.into(),
        );
        pool.insert(
            name.clone(),
            Symbol::Enum(Enum {
                name,
                docstring: None,
                variants: vec![],
                origin: Origin {
                    source_file_path: "test.baml".into(),
                    span_start: 0,
                },
            }),
        );
    }
    let files = to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
    let root_types = &files[&PathBuf::from("baml_sdk/__root.rb")];
    assert!(root_types.contains("class BYTECODE_ <"), "{root_types}");
    assert!(root_types.contains("class METADATA_ <"), "{root_types}");
    assert!(files.contains_key(&PathBuf::from("baml_sdk/nested/__vendor.rb")));
    assert!(
        files
            .keys()
            .all(|path| !path.components().any(|part| part.as_os_str() == "vendor"))
    );
}
