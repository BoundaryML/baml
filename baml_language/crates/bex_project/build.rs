use std::{collections::BTreeMap, path::Path};

use baml_base::Name;
use baml_compiler2_emit::generate_stdlib_program;
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_project::ProjectDatabase;

#[path = "src/precompiled_stdlib_config.rs"]
mod precompiled_stdlib_config;

fn main() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("<precompiled-stdlib>"));

    let interfaces = baml_builtins2::stdlib_package_names()
        .iter()
        .map(|name| {
            let package = PackageId::new(&db, Name::new(*name));
            let bytes = borsh::to_vec(package_interface(&db, package))
                .expect("serialize compiler-built stdlib PackageInterface");
            ((*name).to_string(), bytes)
        })
        .collect::<BTreeMap<_, _>>();
    let program = generate_stdlib_program(&db, precompiled_stdlib_config::OPT_LEVEL)
        .expect("compile compiler-built stdlib bytecode prefix");
    let artifact = (
        precompiled_stdlib_config::artifact_key(),
        interfaces,
        program,
    );
    let bytes = borsh::to_vec(&artifact).expect("serialize compiler-built stdlib artifact");

    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    std::fs::write(
        std::path::PathBuf::from(out_dir).join("stdlib_prefix.borsh"),
        bytes,
    )
    .expect("write compiler-built stdlib artifact");
}
