//! Stable generated-program identity, shared by code generators and bridges.
use sha2::{Digest, Sha256};

/// Canonical compiled contents, excluding artifact headers and diagnostic locations.
/// Borsh orders HashMap keys. Pool order and package order are compiler-defined.
pub fn canonical_bytes(bytecode: &[u8]) -> Result<Vec<u8>, String> {
    let mut program: bex_vm_types::Program =
        baml_artifact::decode(baml_artifact::ArtifactKind::Program, bytecode)
            .map_err(|e| e.to_string())?;
    for object in &mut program.objects {
        if let bex_vm_types::Object::Function(function) = object {
            function.source_file.clear();
            function.span.file_id = Default::default();
            function.debug_locals.clear();
            function.bytecode.line_table.clear();
            function.bytecode.meta.clear();
        }
    }
    borsh::to_vec(&program).map_err(|e| e.to_string())
}

pub fn key_from_canonical(canonical: &[u8]) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"baml.generated-program.v1\0");
    hash.update(canonical);
    let digest = hash.finalize();
    u64::from_be_bytes(digest[..8].try_into().unwrap()) | (1 << 63)
}

pub fn program_key(bytecode: &[u8]) -> Result<u64, String> {
    canonical_bytes(bytecode).map(|bytes| key_from_canonical(&bytes))
}

/// Canonical input for the legacy source-emitting Python generator. Paths must
/// be program-relative, so moving an SDK never changes its registration.
pub fn canonical_sources<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Vec<u8>, String> {
    let mut files: Vec<_> = files
        .into_iter()
        .map(|(path, source)| (path.replace('\\', "/"), source))
        .collect();
    files.sort_unstable();
    let mut bytes = b"baml.generated-sources.v1\0".to_vec();
    for (path, source) in files {
        if path.starts_with('/') || path.contains(':') || path.split('/').any(|part| part == "..") {
            return Err("Generated BAML source paths must be relative to the program root".into());
        }
        for value in [path.as_bytes(), source.as_bytes()] {
            bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
            bytes.extend_from_slice(value);
        }
    }
    Ok(bytes)
}
