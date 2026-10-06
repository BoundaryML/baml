use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use baml_builtins2_codegen::{BamlType, NativeBuiltin, NativeClassDef};

fn type_classes(ty: &BamlType, classes: &[NativeClassDef], out: &mut BTreeSet<String>) {
    match ty {
        BamlType::Media(name) => {
            out.insert(format!("baml.media.{name}"));
        }
        BamlType::Named(name) => {
            for c in classes {
                let fqn = format!("{}.{}", c.namespace_prefix, c.name);
                if c.name == *name
                    || fqn == *name
                    || fqn.strip_prefix("baml.") == Some(name.as_str())
                {
                    out.insert(fqn);
                }
            }
        }
        BamlType::List(t) | BamlType::Optional(t) => type_classes(t, classes, out),
        BamlType::Map(k, v) => {
            type_classes(k, classes, out);
            type_classes(v, classes, out);
        }
        BamlType::String
        | BamlType::Int
        | BamlType::Bigint
        | BamlType::Float
        | BamlType::Bool
        | BamlType::Null
        | BamlType::Generic(_)
        | BamlType::Uint8Array
        | BamlType::RustType => {}
    }
}

fn deps(b: &NativeBuiltin, classes: &[NativeClassDef]) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    if let Some(receiver) = &b.receiver {
        if receiver.instance_backed {
            let package = b.path.split('.').next().unwrap();
            let ns = if receiver.namespace.is_empty() {
                package.into()
            } else {
                format!("{package}.{}", receiver.namespace)
            };
            deps.insert(format!("{ns}.{}", receiver.class_name));
        }
    }
    type_classes(&b.return_type, classes, &mut deps);
    for p in &b.params {
        type_classes(&p.ty, classes, &mut deps);
    }
    deps
}

fn main() {
    println!("cargo:rerun-if-changed=src/native-keys.txt");
    let audited: BTreeSet<_> = include_str!("src/native-keys.txt")
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    let mut contracts = BTreeMap::new();
    for package in ["baml", "ai", "reflect", "testing", "trace"] {
        let (vm, io, classes) =
            baml_builtins2_codegen::extract_native_builtins_for(package).unwrap();
        for b in vm.iter().chain(&io) {
            if audited.contains(b.path.as_str()) {
                contracts.insert(b.path.clone(), deps(b, &classes));
            }
        }
    }
    // Stale keys cannot quietly continue to advertise an obsolete contract.
    for key in &audited {
        assert!(
            contracts.contains_key(*key),
            "stale native pruning key {key}"
        );
    }
    let mut code = String::from(
        "fn generated_dependencies(key: &str) -> Option<&'static [&'static str]> { match key {\n",
    );
    for (key, deps) in &contracts {
        writeln!(
            code,
            "{key:?} => Some(&{:?}),",
            deps.iter().collect::<Vec<_>>()
        )
        .unwrap();
    }
    code.push_str("_ => None, } }\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("native_dependencies.rs"),
        code,
    )
    .unwrap();
}
