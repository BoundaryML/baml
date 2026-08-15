use baml_artifact::{
    BYTECODE_ABI, COMPILER_BUILD_ID, Error, PackageCode, load_linked_program, load_package_codes,
    load_package_interfaces,
};
use baml_base::Name;
use baml_package_interface::PackageInterface;

pub struct EmbeddedArtifact {
    bytes: &'static [u8],
}

static ARTIFACTS: &[EmbeddedArtifact] = include!(concat!(env!("OUT_DIR"), "/artifacts.rs"));

pub fn package_interfaces() -> Result<std::collections::BTreeMap<Name, PackageInterface>, Error> {
    load_package_interfaces(
        ARTIFACTS.iter().map(|artifact| artifact.bytes),
        COMPILER_BUILD_ID,
        BYTECODE_ABI,
    )
}

pub fn linked_program() -> Result<bex_vm_types::Program, Error> {
    load_linked_program(
        ARTIFACTS.iter().map(|artifact| artifact.bytes),
        COMPILER_BUILD_ID,
        BYTECODE_ABI,
    )
}

pub fn package_codes() -> Result<Vec<(Name, PackageCode)>, Error> {
    load_package_codes(
        ARTIFACTS.iter().map(|artifact| artifact.bytes),
        COMPILER_BUILD_ID,
        BYTECODE_ABI,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn linked_program_exports_stdlib_functions_and_packages() {
        let program = super::linked_program().unwrap();
        assert!(
            program
                .function_global_indices
                .contains_key("baml.json.stringify")
        );
        assert!(
            program
                .function_global_indices
                .contains_key("baml.Summable$for$int[].sum")
        );
        assert!(program.packages.contains_key("baml"));
    }
}
