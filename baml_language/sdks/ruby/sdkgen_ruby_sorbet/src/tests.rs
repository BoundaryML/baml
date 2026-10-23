use super::*;
use crate::names::{constant, identifier};
use baml_sdkgen_types::Ty;

#[test]
fn ruby_names_and_container_types() {
    assert_eq!(identifier("class", NamingConvention::Language), "class_");
    assert_eq!(
        identifier("HTTPCall", NamingConvention::Language),
        "http_call"
    );
    assert_eq!(
        identifier("SomeCall", NamingConvention::PreserveCase),
        "SomeCall"
    );
    assert_eq!(constant("throws_test"), "ThrowsTest");
    assert_eq!(
        identifier("method", NamingConvention::Language),
        "baml_method"
    );
    assert_eq!(identifier("sig", NamingConvention::Language), "baml_sig");
    let mut names = NameTable::default();
    assert_eq!(
        names.claim("BamlSdk", "hello_world", "user.HelloWorld"),
        "hello_world"
    );
    assert_eq!(
        names.claim("BamlSdk", "hello_world", "user.HelloWorld"),
        "hello_world"
    );
    assert_eq!(
        names.claim("BamlSdk", "hello_world", "user.hello_world"),
        "hello_world_"
    );
    assert_eq!(
        names.claim("BamlSdk", "hello_world_", "user.hello_world_"),
        "hello_world__"
    );
    assert_eq!(
        names.claim("BamlSdk::Other", "hello_world", "user.other.hello_world"),
        "hello_world"
    );
    assert_eq!(constant("StreamE2ECollectResult"), "StreamE2ECollectResult");
    assert_eq!(constant("_private"), "BamlPrivate");
    assert_eq!(
        ruby_type(&Ty::Never, &BTreeSet::new(), &names).as_deref(),
        Some("::T.noreturn")
    );
    let string = Ty::String;
    let optional = Ty::Union(Box::new([string.clone(), Ty::Null]));
    let map = Ty::Map {
        key: Box::new(string),
        value: Box::new(Ty::List(Box::new(optional))),
    };
    assert_eq!(
        ruby_type(&map, &BTreeSet::new(), &names).as_deref(),
        Some("::T::Hash[::String, ::T::Array[::T.nilable(::String)]]")
    );
    let union = Ty::Union(Box::new([Ty::Int, Ty::Bool]));
    assert_eq!(
        ruby_type(&union, &BTreeSet::new(), &names).as_deref(),
        Some("::T.any(::Integer, ::T::Boolean)")
    );
    assert_eq!(ruby_type(&Ty::Unknown, &BTreeSet::new(), &names), None);
    let parameter = Ty::TypeVar(baml_sdkgen_types::ParamTy::new(1, "foo_bar".into()));
    let generic_union = Ty::Union(Box::new([parameter.clone(), Ty::String, Ty::Null]));
    assert_eq!(
        crate::types::ruby_type_with_params(
            &generic_union,
            &BTreeSet::new(),
            &names,
            &[
                "BamlTypeParam0".into(),
                "::T.type_parameter(:BamlTypeParam1)".into()
            ],
        ),
        Some("::T.any(::T.type_parameter(:BamlTypeParam1), ::String, ::NilClass)".into())
    );
    assert_eq!(
        crate::descriptors::descriptor(&parameter),
        "[:param, 1, \"foo_bar\"].freeze"
    );
}

#[test]
fn stream_and_spec_types_use_runtime_handles() {
    let names = NameTable::default();
    let supported = BTreeSet::new();
    let stream = Name::new("ai".into(), vec!["stream".into()], "Stream".into());
    let spec = Name::new("ai".into(), vec![], "FunctionSpec".into());
    assert_eq!(
        ruby_type(
            &Ty::Class(stream.clone(), Box::new([Ty::String])),
            &supported,
            &names
        ),
        Some("::Baml::Stream[::String]".into())
    );
    assert_eq!(
        ruby_type(&Ty::Class(spec, Box::new([Ty::String])), &supported, &names),
        Some("::Baml::FunctionSpec".into())
    );
    assert_eq!(
        ruby_type(
            &Ty::Class(stream, Box::new([Ty::Unknown])),
            &supported,
            &names
        ),
        None
    );
    // Matching uses the whole BAML name, never just a class's leaf name.
    let user = Name::new("user".into(), vec![], "Stream".into());
    assert_eq!(
        ruby_type(&Ty::Class(user, Box::new([Ty::String])), &supported, &names),
        None
    );
}

#[test]
fn callback_types_use_proc_signatures() {
    let names = NameTable::default();
    let callback = Ty::Function {
        params: Box::new([baml_sdkgen_types::CallableParam::required(None, Ty::Int)]),
        ret: Box::new(Ty::String),
        throws: Box::new(Ty::Never),
    };
    assert_eq!(
        ruby_type(&callback, &BTreeSet::new(), &names).as_deref(),
        Some("::T.any(::T.proc.params(arg0: ::Integer).returns(::String), ::Baml::Closure)")
    );
    assert_eq!(
        crate::descriptors::descriptor(&callback),
        "[:function, [[:int].freeze].freeze, [:string].freeze].freeze"
    );

    // Aliases must retain the same callable-object runtime-check exemption.
    let alias_name = Name::new("user".into(), vec![], "Callback".into());
    let function_name = Name::new("user".into(), vec![], "apply".into());
    let origin = baml_sdkgen_types::Origin {
        source_file_path: "callbacks.baml".into(),
        span_start: 0,
    };
    let pool = SymbolPool::from([
        (
            alias_name.clone(),
            Symbol::TypeAlias(baml_sdkgen_types::TypeAlias {
                name: alias_name.clone(),
                resolves_to: callback,
                recursive: false,
                origin: origin.clone(),
            }),
        ),
        (
            function_name,
            Symbol::Function(baml_sdkgen_types::Function {
                name: "apply".into(),
                generic_params: vec![],
                docstring: None,
                arguments: vec![baml_sdkgen_types::FunctionArgument {
                    name: "callback".into(),
                    docstring: None,
                    ty: Ty::TypeAlias(alias_name.clone()),
                    default: None,
                    injected: false,
                }],
                return_type: Ty::TypeAlias(alias_name),
                throws: None,
                watchers: vec![],
                origin,
            }),
        ),
    ]);
    let files = to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
    let source = &files[&PathBuf::from("baml_sdk/__root.rb")];
    assert_eq!(source.matches(".checked(:never)").count(), 2, "{source}");
    assert!(!files[&PathBuf::from("baml_sdk.rb")].contains("signature_for_method"));
}

#[test]
fn colliding_constants_and_namespaces_keep_wire_identities() {
    use baml_sdkgen_types::{Enum, EnumVariant, Origin};

    let declarations: Vec<_> = [
        (vec![], "fooBar"),
        (vec!["fooBar"], "fooBar"),
        (vec!["fooBar"], "foo_bar"),
        (vec!["foo_bar"], "Choice"),
    ]
    .into_iter()
    .map(|(namespace, leaf)| {
        let name = Name::new(
            "user".into(),
            namespace.into_iter().map(Into::into).collect(),
            leaf.into(),
        );
        let symbol = Symbol::Enum(Enum {
            name: name.clone(),
            docstring: None,
            // Deliberately not in allocation order.
            variants: ["foo_bar", "fooBar", "foo__bar"]
                .into_iter()
                .map(|raw| EnumVariant {
                    name: raw.into(),
                    docstring: None,
                    value: raw.into(),
                })
                .collect(),
            origin: Origin {
                source_file_path: "names.baml".into(),
                span_start: 0,
            },
        });
        (name, symbol)
    })
    .collect();
    let pool: SymbolPool = declarations.iter().cloned().collect();
    let names = NameTable::build(&pool, NamingConvention::Language);
    let expected = [
        "BamlSdk::FooBar_",
        "BamlSdk::FooBar::FooBar",
        "BamlSdk::FooBar::FooBar_",
        "BamlSdk::FooBar__::Choice",
    ];
    for ((name, _), expected) in declarations.iter().zip(expected) {
        assert_eq!(names.qualified(name), expected);
        assert_eq!(
            ruby_type(
                &Ty::Enum(name.clone()),
                &pool.keys().cloned().collect(),
                &names
            ),
            Some(format!("::{expected}"))
        );
    }
    let generate = |pool: &SymbolPool| {
        to_source_code_with_bytecode_and_skipped(pool, &[], NamingConvention::Language, None)
    };
    let (files, skipped) = generate(&pool);
    assert!(skipped.is_empty());
    assert_eq!(
        (files.clone(), skipped),
        generate(&declarations.into_iter().rev().collect())
    );
    let registry = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(registry.contains("define_enum(\"user.fooBar.foo_bar\", ::BamlSdk::FooBar::FooBar_)"));
    let source = &files[&PathBuf::from("baml_sdk/foo_bar.rb")];
    for expected in [
        "FooBar = new(\"fooBar\")",
        "FooBar_ = new(\"foo__bar\")",
        "FooBar__ = new(\"foo_bar\")",
    ] {
        assert!(source.contains(expected), "{source}");
    }
}

fn namespace_pool(namespaces: &[&str]) -> SymbolPool {
    namespaces
        .iter()
        .map(|namespace| {
            let path = if namespace.is_empty() {
                vec![]
            } else {
                vec![(*namespace).into()]
            };
            let name = Name::new("user".into(), path, "Choice".into());
            let symbol = Symbol::Enum(baml_sdkgen_types::Enum {
                name: name.clone(),
                docstring: None,
                variants: vec![],
                origin: baml_sdkgen_types::Origin {
                    source_file_path: "paths.baml".into(),
                    span_start: 0,
                },
            });
            (name, symbol)
        })
        .collect()
}

#[test]
fn namespace_paths_are_snake_case_and_cannot_replace_package_files() {
    let pool = namespace_pool(&["", "root", "bytecode", "throws_test", "throwstest"]);
    let files = to_source_code_with_bytecode(&pool, &[42], NamingConvention::Language, None);
    for namespace in ["root", "bytecode", "throws_test", "throwstest"] {
        let source = &files[&PathBuf::from(format!("baml_sdk/{namespace}.rb"))];
        assert!(source.contains("class Choice"), "{source}");
    }
    assert!(files[&PathBuf::from("baml_sdk/__root.rb")].contains("class Choice"));
    assert!(files[&PathBuf::from("baml_sdk/__bytecode.rb")].contains("BYTECODE ="));
    assert_eq!(files.len(), pool.len() + 2);
}

#[test]
fn vendored_packages_avoid_directories_tapioca_ignores() {
    let name = Name::new("anthropic".into(), vec![], "Choice".into());
    let symbol = Symbol::Enum(baml_sdkgen_types::Enum {
        name: name.clone(),
        docstring: None,
        variants: vec![],
        origin: baml_sdkgen_types::Origin {
            source_file_path: "anthropic.baml".into(),
            span_start: 0,
        },
    });
    let pool: SymbolPool = [(name, symbol)].into_iter().collect();
    let files = to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
    let source = &files[&PathBuf::from("baml_sdk/__vendor/anthropic.rb")];
    assert!(
        source.contains("module BamlSdk::Vendor::Anthropic"),
        "{source}"
    );
    assert!(
        files[&PathBuf::from("baml_sdk.rb")]
            .contains("require_relative \"baml_sdk/__vendor/anthropic\"")
    );
}

#[test]
#[should_panic(expected = "duplicate Ruby output path: baml_sdk/foo_bar.rb")]
fn duplicate_namespace_paths_fail_instead_of_overwriting() {
    let pool = namespace_pool(&["FooBar", "FOOBar"]);
    to_source_code_with_bytecode(&pool, &[], NamingConvention::Language, None);
}

#[test]
fn unsupported_injected_defaults_are_omitted_without_dropping_user_arguments() {
    use baml_sdkgen_types::{Function, FunctionArgument, FunctionArgumentDefault, Origin};

    let function = Function {
        name: "extract".into(),
        generic_params: vec![],
        docstring: None,
        arguments: vec![
            FunctionArgument {
                name: "text".into(),
                docstring: None,
                ty: Ty::String,
                default: None,
                injected: false,
            },
            FunctionArgument {
                // Deliberately not named on_event: only the marker matters.
                name: "callback".into(),
                docstring: None,
                ty: Ty::Unknown,
                default: Some(FunctionArgumentDefault::Null),
                injected: true,
            },
        ],
        return_type: Ty::String,
        throws: None,
        watchers: vec![],
        origin: Origin {
            source_file_path: "test.baml".into(),
            span_start: 0,
        },
    };
    let name = Name::new("user".into(), vec![], "extract".into());
    let generate = |function| {
        to_source_code_with_bytecode_and_skipped(
            &SymbolPool::from([(name.clone(), Symbol::Function(function))]),
            &[],
            NamingConvention::Language,
            None,
        )
    };
    let (files, skipped) = generate(function.clone());
    let source = &files[&PathBuf::from("baml_sdk/__root.rb")];
    assert!(skipped.is_empty());
    assert!(
        source.contains(
            "sig { params(text: ::String, _baml: ::T.nilable(::Baml::Options)).returns(::String) }"
        ),
        "{source}"
    );
    assert!(
        source.contains("def self.extract(text, _baml: nil)"),
        "{source}"
    );
    assert!(
        source.contains(
            "::Baml::Bridge.call_async(\"user.extract\", {\"text\" => text}, baml: _baml).wait"
        ),
        "{source}"
    );
    assert!(!source.contains("callback"), "{source}");

    let mut user_callback = function.clone();
    user_callback.arguments[1].injected = false;
    user_callback.arguments[1].name = "on_event".into();
    let (files, skipped) = generate(user_callback);
    assert_eq!(skipped, ["user.extract"]);
    assert!(!files[&PathBuf::from("baml_sdk/__root.rb")].contains("def self.extract"));

    let mut required_callback = function.clone();
    required_callback.arguments[1].default = None;
    assert_eq!(generate(required_callback).1, ["user.extract"]);

    // Defaulted values are keywords; omission leaves evaluation to BAML.
    for injected in [false, true] {
        let mut supported_default = function.clone();
        supported_default.arguments[1].injected = injected;
        supported_default.arguments[1].ty = Ty::String;
        let (files, skipped) = generate(supported_default);
        let source = &files[&PathBuf::from("baml_sdk/__root.rb")];
        assert!(skipped.is_empty());
        assert!(source.contains("callback: ::Baml::UNSET"));
        assert!(source.contains("callback: ::T.any(::String, ::Baml::Unset)"));
        assert!(source.contains("\"callback\" => callback"));
    }

    // Nullable alone does not make a parameter optional.
    let mut nullable = function;
    nullable.arguments.truncate(1);
    nullable.arguments[0].ty = Ty::Union(vec![Ty::String, Ty::Null].into_boxed_slice());
    let (files, skipped) = generate(nullable);
    assert!(skipped.is_empty());
    let source = &files[&PathBuf::from("baml_sdk/__root.rb")];
    assert!(source.contains("def self.extract(text, _baml: nil)"));
    assert!(source.contains("text: ::T.nilable(::String)"));
    assert!(
        files[&PathBuf::from("baml_sdk.rb")]
            .contains("[\"text\", [:union, [[:string].freeze, [:null].freeze].freeze].freeze]")
    );
    for asynchronous in [false, true] {
        assert!(source.contains(&format!(
            "def self.extract{}(text, _baml: nil)",
            if asynchronous { "_async" } else { "" }
        )));
    }
}

#[test]
fn aliases_preserve_descriptors_and_widen_only_recursive_back_edges() {
    use baml_sdkgen_types::{Origin, TypeAlias};

    let recursive = Name::new("user".into(), vec![], "Rec".into());
    let floats = Name::new("user".into(), vec![], "Floats".into());
    let pool = SymbolPool::from([
        (
            recursive.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: recursive.clone(),
                resolves_to: Ty::Union(Box::new([
                    Ty::Int,
                    Ty::List(Box::new(Ty::TypeAlias(recursive))),
                    Ty::Uint8Array,
                ])),
                recursive: true,
                origin: Origin {
                    source_file_path: "types.baml".into(),
                    span_start: 0,
                },
            }),
        ),
        (
            floats.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: floats.clone(),
                resolves_to: Ty::List(Box::new(Ty::Float)),
                recursive: false,
                origin: Origin {
                    source_file_path: "types.baml".into(),
                    span_start: 1,
                },
            }),
        ),
    ]);
    let supported = pool.keys().cloned().collect();
    let names = NameTable::build(&pool, NamingConvention::Language);
    let ty = Ty::TypeAlias(floats);
    assert_eq!(
        ruby_type(&ty, &supported, &names).as_deref(),
        Some("::BamlSdk::Floats")
    );
    assert_eq!(
        types::ruby_input_type(&ty, &supported, &names, &pool, &[]).as_deref(),
        Some("::T::Array[::T.any(::Float, ::Integer)]")
    );
    let (files, skipped) =
        to_source_code_with_bytecode_and_skipped(&pool, &[], NamingConvention::Language, None);
    assert!(skipped.is_empty());
    let root = &files[&PathBuf::from("baml_sdk.rb")];
    assert!(root.contains("BamlSdk::Rec = ::T.type_alias { ::T.any(::Integer, ::T::Array[::T.untyped], ::String) }"), "{root}");
    assert!(
        root.contains("BamlSdk::Floats = ::T.type_alias { ::T::Array[::Float] }"),
        "{root}"
    );
    assert!(root.contains("::Baml::Bridge.define_alias(\"user.Rec\", [:union, [[:int].freeze, [:list, [:alias, \"user.Rec\"].freeze].freeze, [:bytes].freeze].freeze].freeze)"), "{root}");
}

#[test]
fn bytecode_is_binary_and_generation_is_stable() {
    let first = to_source_code_with_bytecode(
        &SymbolPool::new(),
        &[0, 255, 34, 92],
        NamingConvention::Language,
        None,
    );
    let second = to_source_code_with_bytecode(
        &SymbolPool::new(),
        &[0, 255, 34, 92],
        NamingConvention::Language,
        None,
    );
    assert_eq!(first, second);
    let embedded = baml_sdkgen_types::embedded_bytecode_base64(&[0, 255, 34, 92]);
    assert_eq!(
        first[&PathBuf::from("baml_sdk/__bytecode.rb")],
        format!(
            "# typed: strict\n# frozen_string_literal: true\nmodule BamlSdk\n  BYTECODE = ::T.let(\"{embedded}\".b.freeze, ::String)\n  METADATA = ::T.let(nil, ::T.nilable(::String))\nend\n"
        )
    );
    assert!(embedded.is_ascii() && !embedded.contains(['\n', '\r']));
    assert!(first[&PathBuf::from("baml_sdk.rb")].contains("::Baml::Bridge.initialize!\n"));
}
