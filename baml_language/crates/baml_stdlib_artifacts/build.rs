use std::{collections::BTreeMap, env, fmt::Write as _, fs, path::PathBuf};

use baml_artifact::{
    ArtifactBuilder, BYTECODE_ABI, COMPILER_BUILD_ID, DependencyRequirement, hash,
    load_linked_program,
};
use baml_base::Name;
use baml_compiler2_emit::{CompileOptions, OptLevel, emit_units};
use baml_compiler2_hir::package::{PackageId, package_dependencies};
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_project::ProjectDatabase;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut db = ProjectDatabase::new();
    db.set_project_root(PathBuf::from("<stdlib-artifact-build>").as_path());

    let package_names = baml_builtins2::stdlib_package_names();
    let interfaces = package_names
        .iter()
        .map(|name| {
            let name = Name::new(*name);
            let package_id = PackageId::new(&db, name.clone());
            (name, package_interface(&db, package_id).clone())
        })
        .collect::<BTreeMap<_, _>>();
    let interface_hashes = interfaces
        .iter()
        .map(|(name, interface)| {
            let bytes = borsh::to_vec(interface).unwrap();
            (name.clone(), hash(&bytes))
        })
        .collect::<BTreeMap<_, _>>();
    let mut units_by_package = emit_units(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
    )
    .unwrap()
    .into_iter()
    .fold(BTreeMap::<Name, Vec<_>>::new(), |mut packages, unit| {
        packages.entry(unit.package.clone()).or_default().push(unit);
        packages
    });

    let mut generated = String::from("&[\n");
    let mut artifact_bytes = Vec::new();
    for (index, (name, interface)) in interfaces.iter().enumerate() {
        let package_id = PackageId::new(&db, name.clone());
        let dependencies = package_dependencies(&db, package_id)
            .iter()
            .filter_map(|dependency| {
                let dependency_name = dependency.name(&db);
                interface_hashes
                    .get(&dependency_name)
                    .map(|interface_hash| DependencyRequirement {
                        name: dependency_name,
                        interface_hash: *interface_hash,
                    })
            })
            .collect();
        let source_hash = package_source_hash(name);
        let bytes =
            ArtifactBuilder::new(COMPILER_BUILD_ID, BYTECODE_ABI, name.clone(), source_hash)
                .dependencies(dependencies)
                .interface(interface)
                .unwrap()
                .code(&baml_artifact::PackageCode {
                    units: units_by_package.remove(name).unwrap_or_default(),
                })
                .unwrap()
                .finish()
                .unwrap();
        let filename = format!("package-{index}.baml-artifact");
        fs::write(out_dir.join(&filename), &bytes).unwrap();
        artifact_bytes.push(bytes);
        writeln!(
            generated,
            "EmbeddedArtifact {{ bytes: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{filename}\")) }},"
        )
        .unwrap();
    }
    let linked = load_linked_program(
        artifact_bytes.iter().map(Vec::as_slice),
        COMPILER_BUILD_ID,
        BYTECODE_ABI,
    )
    .unwrap();
    let expected = baml_compiler2_emit::generate_stdlib_program(&db, OptLevel::Two).unwrap();
    assert_eq!(linked.objects.len(), expected.objects.len());
    assert_eq!(
        linked
            .function_indices
            .keys()
            .collect::<std::collections::BTreeSet<_>>(),
        expected
            .function_indices
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
    );
    generated.push_str("]\n");
    fs::write(out_dir.join("artifacts.rs"), generated).unwrap();
}

fn package_source_hash(package: &Name) -> [u8; 32] {
    let mut bytes = Vec::new();
    for builtin in baml_builtins2::ALL
        .iter()
        .filter(|builtin| builtin.package == package.as_str())
    {
        bytes.extend_from_slice(&(builtin.relative_path.len() as u64).to_le_bytes());
        bytes.extend_from_slice(builtin.relative_path.as_bytes());
        bytes.extend_from_slice(&(builtin.contents.len() as u64).to_le_bytes());
        bytes.extend_from_slice(builtin.contents.as_bytes());
    }
    hash(&bytes)
}
