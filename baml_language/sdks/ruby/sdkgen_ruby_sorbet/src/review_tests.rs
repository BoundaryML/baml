use super::*;
use baml_sdkgen_types::{Enum, EnumVariant, Function, Origin, Ty};

#[test]
fn skips_report_the_users_package_not_a_hardcoded_user_prefix() {
    let mut pool = SymbolPool::new();
    for package in ["my_app", "dependency"] {
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
        None,
        "my_app",
    );
    assert_eq!(warnings.len(), 2);
    for warning in warnings {
        assert_eq!(warning.local, warning.fqn.starts_with("my_app."));
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
