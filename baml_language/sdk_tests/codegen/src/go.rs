//! Go SDK test codegen.
//!
//! Stages the shared fixture corpus — whose supported behavior mirrors the
//! Python pydantic2 tests, plus an `unsupported_only` package proving deferred
//! symbols leave valid Go behind — and one synthetic `package_edges` fixture
//! for Go-specific cross-package import collisions. `package_edges` has no
//! `baml_src`: it is built from a hand-constructed [`SymbolPool`] below,
//! because the collisions it exercises are not expressible in BAML source.

use std::{collections::HashMap, fs, path::PathBuf};

use baml_base::{Literal, Name as BaseName};
use baml_codegen_types::{
    CallableParam, Class, ClassProperty, CodegenFunctionParamMode, DefaultLiteral, Enum,
    EnumVariant, Function, FunctionArgument, FunctionArgumentDefault, Name, NamingConvention,
    Origin, Symbol, SymbolPool, Ty, TypeAlias,
};
use baml_type::TyAttr;
use sdk_test_harness_runner::fixtures;

use crate::{CodegenCtx, copy_customizable, load_fixture, write_codegen_output};

const RUNTIME_GO_SUM: &str = include_str!("../../../sdks/go/baml_go/go.sum");

fn ty_string() -> Ty {
    Ty::String {
        attr: TyAttr::default(),
    }
}

fn ty_int() -> Ty {
    Ty::Int {
        attr: TyAttr::default(),
    }
}

fn ty_bool() -> Ty {
    Ty::Bool {
        attr: TyAttr::default(),
    }
}

fn ty_enum(name: Name) -> Ty {
    Ty::Enum(name, TyAttr::default())
}

fn ty_class(name: Name, arguments: Vec<Ty>) -> Ty {
    Ty::Class(name, arguments.into(), TyAttr::default())
}

fn ty_alias(name: Name) -> Ty {
    Ty::TypeAlias(name, TyAttr::default())
}

fn ty_union(members: Vec<Ty>) -> Ty {
    Ty::Union(members.into(), TyAttr::default())
}

fn ty_callable(params: Vec<Ty>, ret: Ty) -> Ty {
    Ty::Function {
        params: params
            .into_iter()
            .map(|ty| CallableParam {
                name: None,
                ty,
                mode: CodegenFunctionParamMode::Required,
            })
            .collect(),
        ret: Box::new(ret),
        throws: Box::new(Ty::Never {
            attr: TyAttr::default(),
        }),
        attr: TyAttr::default(),
    }
}

/// Generate every Go fixture SDK. Called by the `go` subcommand of the
/// `sdk_test_codegen` binary, which `crates/go/setup.sh` runs before
/// `go mod tidy`.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the Go suite, so a panic here should stop the setup
/// script outright instead of surfacing later as a separate test.
pub fn run_all(ctx: &CodegenCtx) {
    let discovered = fixtures::discover_shared(&ctx.fixtures_root);
    assert_eq!(
        discovered,
        fixtures::SHARED,
        "the fixture corpus at {} has drifted from `fixtures::SHARED`",
        ctx.fixtures_root.display()
    );

    for fixture in fixtures::SHARED {
        let loaded = load_fixture(&ctx.fixtures_root, fixture);
        let output = sdkgen_go::to_source_code_with_bytecode(
            &loaded.pool,
            &loaded.baml_bytecode,
            NamingConvention::Language,
            "baml.local/sdk/baml_sdk",
        );
        stage_output(&ctx.crate_dir, fixture, output);
    }
    stage_package_edges(&ctx.crate_dir);
}

fn stage_output(crate_dir: &std::path::Path, fixture: &str, output: HashMap<PathBuf, String>) {
    let fixture_root = crate_dir.join(fixture);
    let generated = fixture_root.join("generated");
    let sdk = generated.join("baml_sdk");
    if generated.exists() {
        // Runtime calls and direct Go validation may create `.baml/` state and
        // `target/` cache directories in the generated module. They are not
        // generator output and can carry platform metadata or read-only cache
        // entries, so preserve them while clearing files owned by staging.
        for entry in fs::read_dir(&generated).unwrap() {
            let entry = entry.unwrap();
            if matches!(entry.file_name().to_str(), Some(".baml" | "target")) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(path).unwrap();
            } else {
                fs::remove_file(path).unwrap();
            }
        }
    }
    write_codegen_output(&sdk, output, fixture);

    let customizable = fixture_root.join("customizable");
    if customizable.exists() {
        copy_customizable(&customizable, &generated);
    }
    fs::write(
        generated.join("go.mod"),
        "module baml.local/sdk\n\ngo 1.23\n\nrequire github.com/boundaryml/baml-go v0.0.0\n\nrequire google.golang.org/protobuf v1.36.6 // indirect\n\nreplace github.com/boundaryml/baml-go => ../../../../../sdks/go/baml_go\n",
    )
    .unwrap();
    fs::write(generated.join("go.sum"), RUNTIME_GO_SUM).unwrap();
}

fn stage_package_edges(crate_dir: &std::path::Path) {
    let context = Name::new(BaseName::new("context"), vec![], BaseName::new("Thing"));
    let dashed = Name::new(BaseName::new("foo-bar"), vec![], BaseName::new("Thing"));
    let snake = Name::new(BaseName::new("foo_bar"), vec![], BaseName::new("Thing"));
    let models = Name::new(BaseName::new("models"), vec![], BaseName::new("Thing"));
    let status = Name::new(BaseName::new("models"), vec![], BaseName::new("Status"));
    let status_alias = Name::new(
        BaseName::new("models"),
        vec![],
        BaseName::new("StatusAlias"),
    );
    let thing_alias = Name::new(BaseName::new("models"), vec![], BaseName::new("ThingAlias"));
    let holder = Name::new(BaseName::new("user"), vec![], BaseName::new("Holder"));
    let envelope = Name::new(BaseName::new("user"), vec![], BaseName::new("Envelope"));
    let enum_holder = Name::new(BaseName::new("user"), vec![], BaseName::new("EnumHolder"));
    let round_trip = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("round_trip_context_thing"),
    );
    let shadowed_package = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("round_trip_models_thing"),
    );
    let nested_packages = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("round_trip_envelope"),
    );
    let enum_round_trip = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("round_trip_enum_holder"),
    );
    let status_alias_round_trip = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("round_trip_status_alias"),
    );
    let thing_alias_round_trip = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("round_trip_thing_alias"),
    );
    let union_callback = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("call_cross_package_union_callback"),
    );
    let defaulted_extract = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("defaulted_extract"),
    );
    let defaulted_extract_spec = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("defaulted_extract@spec"),
    );
    let defaulted_extract_stream = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("defaulted_extract@stream"),
    );
    let static_factory = Name::new(
        BaseName::new("user"),
        vec![],
        BaseName::new("StaticFactory"),
    );
    let static_methods = vec![
        synthetic_method(
            "round_trip_model",
            vec![("value", ty_class(models.clone(), vec![]), false)],
            ty_class(models.clone(), vec![]),
        ),
        synthetic_method(
            "round_trip_alias",
            vec![("value", ty_alias(thing_alias.clone()), false)],
            ty_alias(thing_alias.clone()),
        ),
        synthetic_method(
            "round_trip_enum",
            vec![("value", ty_alias(status_alias.clone()), true)],
            ty_alias(status_alias.clone()),
        ),
        synthetic_method(
            "round_trip_nested",
            vec![("value", ty_class(envelope.clone(), vec![]), false)],
            ty_class(envelope.clone(), vec![]),
        ),
        synthetic_method(
            "unsupported_media",
            vec![(
                "value",
                Ty::Media(baml_base::MediaKind::Generic, TyAttr::default()),
                false,
            )],
            Ty::Media(baml_base::MediaKind::Generic, TyAttr::default()),
        ),
    ];
    let pool = SymbolPool::from([
        synthetic_class(context.clone(), vec![("value", ty_string())]),
        synthetic_class(dashed.clone(), vec![("value", ty_int())]),
        synthetic_class(snake.clone(), vec![("value", ty_bool())]),
        synthetic_class(models.clone(), vec![("value", ty_string())]),
        synthetic_enum(status.clone(), &["ready", "done"]),
        synthetic_type_alias(status_alias.clone(), ty_enum(status), false),
        synthetic_type_alias(thing_alias.clone(), ty_class(models.clone(), vec![]), false),
        synthetic_class(
            holder.clone(),
            vec![
                ("context_thing", ty_class(context.clone(), vec![])),
                ("dashed_thing", ty_class(dashed, vec![])),
                ("snake_thing", ty_class(snake, vec![])),
            ],
        ),
        synthetic_class(envelope.clone(), vec![("holder", ty_class(holder, vec![]))]),
        synthetic_class(
            enum_holder.clone(),
            vec![("status", ty_alias(status_alias.clone()))],
        ),
        (
            round_trip,
            Symbol::Function(Function {
                name: BaseName::new("round_trip_context_thing"),
                generic_params: vec![],
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: ty_class(context.clone(), vec![]),
                    default: None,
                }],
                return_type: ty_class(context, vec![]),
                throws: None,
                watchers: vec![],
                origin: Origin {
                    source_file_path: "synthetic.baml".to_string(),
                    span_start: 0,
                },
            }),
        ),
        (
            shadowed_package,
            Symbol::Function(Function {
                name: BaseName::new("round_trip_models_thing"),
                generic_params: vec![],
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("models"),
                        docstring: None,
                        ty: ty_string(),
                        default: None,
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("value"),
                        docstring: None,
                        ty: ty_class(models.clone(), vec![]),
                        default: None,
                    },
                ],
                return_type: ty_class(models.clone(), vec![]),
                throws: None,
                watchers: vec![],
                origin: Origin {
                    source_file_path: "synthetic.baml".to_string(),
                    span_start: 0,
                },
            }),
        ),
        (
            nested_packages,
            Symbol::Function(Function {
                name: BaseName::new("round_trip_envelope"),
                generic_params: vec![],
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: ty_class(envelope.clone(), vec![]),
                    default: None,
                }],
                return_type: ty_class(envelope, vec![]),
                throws: None,
                watchers: vec![],
                origin: Origin {
                    source_file_path: "synthetic.baml".to_string(),
                    span_start: 0,
                },
            }),
        ),
        (
            enum_round_trip,
            Symbol::Function(Function {
                name: BaseName::new("round_trip_enum_holder"),
                generic_params: vec![],
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: ty_class(enum_holder.clone(), vec![]),
                    default: None,
                }],
                return_type: ty_class(enum_holder, vec![]),
                throws: None,
                watchers: vec![],
                origin: Origin {
                    source_file_path: "synthetic.baml".to_string(),
                    span_start: 0,
                },
            }),
        ),
        round_trip_function(status_alias_round_trip, ty_alias(status_alias)),
        round_trip_function(thing_alias_round_trip, ty_alias(thing_alias)),
        (
            union_callback,
            Symbol::Function(Function {
                name: BaseName::new("call_cross_package_union_callback"),
                generic_params: vec![],
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("callback"),
                        docstring: None,
                        ty: ty_callable(
                            vec![ty_union(vec![
                                ty_string(),
                                ty_class(models.clone(), vec![]),
                            ])],
                            ty_union(vec![ty_string(), ty_class(models.clone(), vec![])]),
                        ),
                        default: None,
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("value"),
                        docstring: None,
                        ty: ty_union(vec![ty_string(), ty_class(models.clone(), vec![])]),
                        default: None,
                    },
                ],
                return_type: ty_union(vec![ty_string(), ty_class(models, vec![])]),
                throws: None,
                watchers: vec![],
                origin: Origin {
                    source_file_path: "synthetic.baml".to_string(),
                    span_start: 0,
                },
            }),
        ),
        synthetic_defaulted_extract(defaulted_extract, "defaulted_extract", ty_string(), false),
        synthetic_defaulted_extract(
            defaulted_extract_spec,
            "defaulted_extract@spec",
            ty_class(
                Name::new(BaseName::new("ai"), vec![], BaseName::new("FunctionSpec")),
                vec![ty_string()],
            ),
            false,
        ),
        synthetic_defaulted_extract(
            defaulted_extract_stream,
            "defaulted_extract@stream",
            ty_class(
                Name::new(
                    BaseName::new("ai"),
                    vec![BaseName::new("stream")],
                    BaseName::new("Stream"),
                ),
                vec![ty_string(), ty_string()],
            ),
            true,
        ),
        synthetic_class_with_methods(static_factory, vec![], static_methods),
    ]);
    let output = sdkgen_go::to_source_code_with_bytecode(
        &pool,
        &[],
        NamingConvention::Language,
        "baml.local/sdk/baml_sdk",
    );
    stage_output(crate_dir, "package_edges", output);
}

fn synthetic_defaulted_extract(
    name: Name,
    function_name: &str,
    return_type: Ty,
    include_on_event: bool,
) -> (Name, Symbol) {
    let mut arguments = vec![
        FunctionArgument {
            injected: false,
            name: BaseName::new("text"),
            docstring: None,
            ty: ty_string(),
            default: None,
        },
        FunctionArgument {
            injected: false,
            name: BaseName::new("tone"),
            docstring: None,
            ty: ty_string(),
            default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                Literal::String("neutral".to_string()),
            ))),
        },
    ];
    if include_on_event {
        arguments.push(FunctionArgument {
            injected: true,
            name: BaseName::new("on_event"),
            docstring: None,
            ty: ty_union(vec![
                ty_callable(
                    vec![ty_string()],
                    Ty::Void {
                        attr: TyAttr::default(),
                    },
                ),
                Ty::Null {
                    attr: TyAttr::default(),
                },
            ]),
            default: Some(FunctionArgumentDefault::Null),
        });
    }
    let function = Function {
        name: BaseName::new(function_name),
        generic_params: vec![],
        docstring: None,
        arguments,
        return_type,
        throws: None,
        watchers: vec![],
        origin: Origin {
            source_file_path: "synthetic.baml".to_string(),
            span_start: 0,
        },
    };
    (name, Symbol::Function(function))
}

fn synthetic_class(name: Name, properties: Vec<(&str, Ty)>) -> (Name, Symbol) {
    synthetic_class_with_methods(name, properties, vec![])
}

fn synthetic_class_with_methods(
    name: Name,
    properties: Vec<(&str, Ty)>,
    static_methods: Vec<Function>,
) -> (Name, Symbol) {
    let class = Class {
        name: name.clone(),
        generic_params: vec![],
        docstring: None,
        properties: properties
            .into_iter()
            .map(|(name, ty)| ClassProperty {
                name: BaseName::new(name),
                docstring: None,
                ty,
            })
            .collect(),
        static_methods,
        instance_methods: vec![],
        origin: Origin {
            source_file_path: "synthetic.baml".to_string(),
            span_start: 0,
        },
    };
    (name, Symbol::Class(class))
}

fn synthetic_method(name: &str, arguments: Vec<(&str, Ty, bool)>, return_type: Ty) -> Function {
    Function {
        name: BaseName::new(name),
        generic_params: vec![],
        docstring: None,
        arguments: arguments
            .into_iter()
            .map(|(name, ty, defaulted)| FunctionArgument {
                injected: false,
                name: BaseName::new(name),
                docstring: None,
                ty,
                default: defaulted.then_some(
                    baml_codegen_types::FunctionArgumentDefault::Expression { source: None },
                ),
            })
            .collect(),
        return_type,
        throws: None,
        watchers: vec![],
        origin: Origin {
            source_file_path: "synthetic.baml".to_string(),
            span_start: 0,
        },
    }
}

fn synthetic_enum(name: Name, variants: &[&str]) -> (Name, Symbol) {
    let enum_ = Enum {
        name: name.clone(),
        docstring: None,
        variants: variants
            .iter()
            .map(|variant| EnumVariant {
                name: BaseName::new(*variant),
                docstring: None,
                value: (*variant).to_string(),
            })
            .collect(),
        origin: Origin {
            source_file_path: "synthetic.baml".to_string(),
            span_start: 0,
        },
    };
    (name, Symbol::Enum(enum_))
}

fn synthetic_type_alias(name: Name, resolves_to: Ty, recursive: bool) -> (Name, Symbol) {
    let alias = TypeAlias {
        name: name.clone(),
        resolves_to,
        recursive,
        origin: Origin {
            source_file_path: "synthetic.baml".to_string(),
            span_start: 0,
        },
    };
    (name, Symbol::TypeAlias(alias))
}

fn round_trip_function(name: Name, ty: Ty) -> (Name, Symbol) {
    let function = Function {
        name: name.name().clone(),
        generic_params: vec![],
        docstring: None,
        arguments: vec![FunctionArgument {
            injected: false,
            name: BaseName::new("value"),
            docstring: None,
            ty: ty.clone(),
            default: None,
        }],
        return_type: ty,
        throws: None,
        watchers: vec![],
        origin: Origin {
            source_file_path: "synthetic.baml".to_string(),
            span_start: 0,
        },
    };
    (name, Symbol::Function(function))
}
