//! The revision dictionary (`.bamldict`) — observability design §4.2.
//!
//! Built as a pure walk over a finalized `Program` (single-digit ms for
//! ~1000 functions), written once per revision to
//! `<project>/.baml/dict/baml_rev_1_<b64url>.bamldict`. Writes are
//! idempotent (content-addressed name; tmp + rename; the rename-race loser
//! is a no-op). A segment header referencing a `revision_id` must never be
//! created before the dictionary's rename returns (§4.2 write ordering —
//! the consumer's `ensure_dict_written` enforces it).

use std::{
    io,
    path::{Path, PathBuf},
};

use bex_vm_types::{
    CaptureOption, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin, LambdaKind,
    Program, RevisionId,
    identity::{
        DefHashResolver, FUNCTION_ID_SPAWN_CLOSURE, FUNCTION_ID_UNKNOWN,
        SPAWN_CLOSURE_DISPLAY_NAME, SPAWN_CLOSURE_FQN, UNKNOWN_FUNCTION_DISPLAY_NAME,
        UNKNOWN_FUNCTION_FQN,
    },
    types::Object,
};
use prost::Message as _;

/// Generated `baml.dict.v1` protobuf types.
#[allow(
    clippy::pedantic,
    clippy::doc_markdown,
    unreachable_pub,
    reason = "prost-generated code"
)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/baml.dict.v1.rs"));
}

/// Names the §7.1 compiled-in capture-default table that produced the
/// per-function `capture_flags`. Bump when the default table changes.
pub const CAPTURE_POLICY_VERSION: u32 = 1;

/// Sentinel `file_id` for functions with no source file.
pub const FILE_ID_NONE: u32 = u32::MAX;

/// §7.1 capture bitfield: bits 0-1 inputs, 2-3 output, 4-5 error, 6-7
/// promote_on_error (0 = Disabled, 1 = Auto, 2 = Enabled), bit 8 `is_llm`,
/// bit 9 `captures_any`.
#[must_use]
pub fn capture_flags(props: FunctionCaptureProps, is_llm: bool) -> u32 {
    const fn bits(option: CaptureOption) -> u32 {
        match option {
            CaptureOption::Disabled => 0,
            CaptureOption::Auto => 1,
            CaptureOption::Enabled => 2,
        }
    }
    let mut flags = bits(props.inputs)
        | (bits(props.output) << 2)
        | (bits(props.error) << 4)
        | (bits(props.promote_on_error) << 6);
    if is_llm {
        flags |= 1 << 8;
    }
    if flags & 0xFF != 0 {
        flags |= 1 << 9;
    }
    flags
}

/// Build the dictionary from a finalized program.
///
/// Returns `None` when the program carries no identity (the caller must
/// finalize first — packs get the §4.3 fallback identity at load, so by
/// dictionary time identity is always present on healthy paths).
#[must_use]
pub fn build_revision_dictionary(program: &Program) -> Option<pb::RevisionDictionaryV1> {
    let identity = program.identity.as_ref()?;
    let fallback_identity = program.source_files.is_empty();

    // File table: the finalizer's source files first (with content hashes),
    // then any function-referenced path not already present (no hash —
    // fallback-identity programs land here).
    let mut file_ids: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut files: Vec<pb::FileRow> = Vec::new();
    for source in &program.source_files {
        let file_id = u32::try_from(files.len()).unwrap_or(FILE_ID_NONE);
        file_ids.insert(source.path.as_str(), file_id);
        files.push(pb::FileRow {
            file_id,
            path: source.path.clone(),
            content_hash: source.content_hash.to_vec(),
        });
    }
    for obj in program.objects.iter() {
        if let Object::Function(func) = obj
            && !func.source_file.is_empty()
            && !file_ids.contains_key(func.source_file.as_str())
        {
            let file_id = u32::try_from(files.len()).unwrap_or(FILE_ID_NONE);
            file_ids.insert(func.source_file.as_str(), file_id);
            files.push(pb::FileRow {
                file_id,
                path: func.source_file.clone(),
                content_hash: Vec::new(),
            });
        }
    }

    // Reserved rows first (§4.1): readers resolve ids 0/1 without special
    // cases.
    let mut functions: Vec<pb::FunctionDictRow> = vec![
        synthetic_row(
            FUNCTION_ID_UNKNOWN,
            UNKNOWN_FUNCTION_FQN,
            UNKNOWN_FUNCTION_DISPLAY_NAME,
        ),
        synthetic_row(
            FUNCTION_ID_SPAWN_CLOSURE,
            SPAWN_CLOSURE_FQN,
            SPAWN_CLOSURE_DISPLAY_NAME,
        ),
    ];

    let resolver = DefHashResolver::new(program);
    for obj in program.objects.iter() {
        if let Object::Function(func) = obj {
            functions.push(function_row(&resolver, func, &file_ids));
        }
    }

    Some(pb::RevisionDictionaryV1 {
        identity: Some(pb::IdentitySection {
            revision_id: identity.revision_id.0.to_vec(),
            source_snapshot_id: identity.source_snapshot_id.0.to_vec(),
            compiler_id: identity.compiler_id.clone(),
            function_count: identity.function_count,
            fallback_identity,
        }),
        capture_policy_version: CAPTURE_POLICY_VERSION,
        files: Some(pb::FileSection { files }),
        functions: Some(pb::FunctionSection { functions }),
        // Emitted (empty) now; populated by the record-slimming milestone.
        call_sites: Some(pb::CallSiteSection {
            call_sites: Vec::new(),
        }),
    })
}

fn synthetic_row(function_id: u32, fqn: &str, display_name: &str) -> pb::FunctionDictRow {
    pb::FunctionDictRow {
        function_id,
        fqn: fqn.to_string(),
        display_name: display_name.to_string(),
        declared_name: None,
        file_id: FILE_ID_NONE,
        span_start: 0,
        span_end: 0,
        line: 0,
        kind: "synthetic".to_string(),
        origin: "internal".to_string(),
        definition_key: String::new(),
        owner_type_key: None,
        lambda: None,
        package_name: None,
        namespace: Vec::new(),
        capture_flags: 0,
        def_content_hash: Vec::new(),
    }
}

fn function_row(
    resolver: &DefHashResolver<'_>,
    func: &Function,
    file_ids: &std::collections::HashMap<&str, u32>,
) -> pb::FunctionDictRow {
    let fqn = func.name.clone();
    // Display name: the declared name when present, else the last dotted
    // segment (lambdas keep their full debug identity — display-only).
    let display_name = func
        .declared_name
        .clone()
        .unwrap_or_else(|| fqn.rsplit('.').next().unwrap_or(&fqn).to_string());
    // Package/namespace split of the FQN (display-level; joins go through
    // definition_key, never these).
    let mut segments: Vec<&str> = fqn.split('.').collect();
    let package_name = if segments.len() > 1 {
        Some(segments.remove(0).to_string())
    } else {
        None
    };
    let namespace: Vec<String> = if segments.len() > 1 {
        segments[..segments.len() - 1]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    } else {
        Vec::new()
    };

    let (definition_key, owner_type_key, lambda) = match &func.def_meta {
        Some(meta) => (
            meta.definition_key.clone(),
            meta.owner_type_key.clone(),
            meta.lambda.as_ref().map(|l| pb::LambdaIdentityRow {
                parent_definition_key: l.parent_definition_key.clone(),
                ordinal: l.ordinal,
                kind: match l.kind {
                    LambdaKind::Lambda => 0,
                    LambdaKind::SpawnedClosure => 1,
                    LambdaKind::Adapter => 2,
                },
            }),
        ),
        None => (String::new(), None, None),
    };

    let is_llm = func.body_meta.is_some();
    let kind = match &func.kind {
        FunctionKind::Bytecode => "bytecode".to_string(),
        FunctionKind::SysOp(op) => format!("sysop:{op:?}"),
        FunctionKind::Native(_) | FunctionKind::NativeUnresolved => "native".to_string(),
    };
    let origin = match func.origin {
        FunctionOrigin::UserDefined => "user",
        FunctionOrigin::Companion => "companion",
        FunctionOrigin::Internal => "internal",
        FunctionOrigin::Builtin => "builtin",
        FunctionOrigin::AutoDerive => "auto_derive",
    }
    .to_string();

    pb::FunctionDictRow {
        function_id: func.function_id,
        fqn,
        display_name,
        declared_name: func.declared_name.clone(),
        file_id: file_ids
            .get(func.source_file.as_str())
            .copied()
            .unwrap_or(FILE_ID_NONE),
        span_start: u32::from(func.span.range.start()),
        span_end: u32::from(func.span.range.end()),
        line: 0,
        kind,
        origin,
        definition_key,
        owner_type_key,
        lambda,
        package_name,
        namespace,
        capture_flags: capture_flags(func.capture, is_llm),
        def_content_hash: resolver.def_content_hash(func).to_vec(),
    }
}

/// The content-addressed dictionary file name for a revision.
#[must_use]
pub fn dict_file_name(revision_id: RevisionId) -> String {
    format!("{}.bamldict", revision_id.encode())
}

/// Idempotently write the dictionary under `dict_dir` (usually
/// `<project>/.baml/dict/`). Existence check → tmp → rename; a concurrent
/// writer racing the rename is a no-op loser. Returns the final path.
pub fn ensure_dict_written(
    dict_dir: &Path,
    dict: &pb::RevisionDictionaryV1,
) -> io::Result<PathBuf> {
    let revision_id = dict
        .identity
        .as_ref()
        .and_then(|identity| <[u8; 32]>::try_from(identity.revision_id.as_slice()).ok())
        .map(RevisionId)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "dictionary has no 32-byte revision id",
            )
        })?;
    let path = dict_dir.join(dict_file_name(revision_id));
    if path.exists() {
        // Content-addressed: same name = same bytes. Nothing to do.
        return Ok(path);
    }
    std::fs::create_dir_all(dict_dir)?;
    let mut bytes = Vec::with_capacity(dict.encoded_len() + 8);
    dict.encode_length_delimited(&mut bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = dict_dir.join(format!(
        ".{}.tmp-{}",
        dict_file_name(revision_id),
        std::process::id()
    ));
    // §4.2 ordering: a segment header referencing this revision_id must
    // never become durable before the dictionary's rename does — so the
    // rename itself must be durable (tmp fsync → rename → dir fsync).
    match crate::fsutil::write_replace_durable(&tmp, &path, &bytes) {
        Ok(()) => Ok(path),
        Err(err) => {
            // Rename-race loser: if the file exists now, someone else won
            // with identical content — success. Otherwise report.
            let _ = std::fs::remove_file(&tmp);
            if path.exists() { Ok(path) } else { Err(err) }
        }
    }
}

/// Parse a `.bamldict` file's bytes.
pub fn read_dict(bytes: &[u8]) -> io::Result<pb::RevisionDictionaryV1> {
    pb::RevisionDictionaryV1::decode_length_delimited(bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

/// Resolve a function id through the dictionary (readers' fallback is
/// `fn#<id>` when the id is absent — never a silent blank).
#[must_use]
pub fn function_row_by_id(
    dict: &pb::RevisionDictionaryV1,
    function_id: u32,
) -> Option<&pb::FunctionDictRow> {
    dict.functions
        .as_ref()?
        .functions
        .iter()
        .find(|row| row.function_id == function_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_vm_types::identity::{
        FIRST_POOL_FUNCTION_ID, ProgramIdentity, SourceSnapshotId, assign_function_ids,
    };

    fn finalized_test_program() -> Program {
        let mut program = Program::default();
        let mut func = test_function("user.Main");
        func.source_file = "main.baml".to_string();
        func.def_meta = Some(bex_vm_types::DefinitionMeta {
            definition_key: "function:user.Main".to_string(),
            owner_type_key: None,
            lambda: None,
        });
        program.objects.push(Object::Function(Box::new(func)));
        let count = assign_function_ids(&mut program);
        program.identity = Some(ProgramIdentity {
            revision_id: RevisionId([3; 32]),
            source_snapshot_id: SourceSnapshotId([4; 32]),
            compiler_id: "0.15.0+test".to_string(),
            function_count: count,
        });
        program.source_files = vec![bex_vm_types::SourceFileIdentity {
            path: "main.baml".to_string(),
            content_hash: [9; 32],
        }];
        program
    }

    fn test_function(name: &str) -> Function {
        Function {
            name: name.to_string(),
            source_file: String::new(),
            docstring: None,
            declared_name: Some(name.rsplit('.').next().unwrap_or(name).to_string()),
            arity: 0,
            real_local_count: 0,
            bytecode: bex_vm_types::Bytecode::default(),
            kind: FunctionKind::Bytecode,
            local_names: vec![],
            debug_locals: vec![],
            span: baml_base::Span::fake(),
            return_type: baml_type::TyTemplate::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            param_names: vec![],
            param_types: vec![],
            param_has_default: vec![],
            display_type_params: vec![],
            display_param_types: vec![],
            display_return_type: String::new(),
            throws_type: baml_type::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            def_meta: None,
            function_id: 0,
        }
    }

    #[test]
    fn dictionary_walk_emits_reserved_rows_first_and_resolves() {
        let program = finalized_test_program();
        let dict = build_revision_dictionary(&program).expect("finalized program");
        let functions = &dict.functions.as_ref().unwrap().functions;
        assert_eq!(functions[0].function_id, FUNCTION_ID_UNKNOWN);
        assert_eq!(functions[0].fqn, UNKNOWN_FUNCTION_FQN);
        assert_eq!(functions[1].function_id, FUNCTION_ID_SPAWN_CLOSURE);
        assert_eq!(functions[2].function_id, FIRST_POOL_FUNCTION_ID);
        assert_eq!(functions[2].fqn, "user.Main");
        assert_eq!(functions[2].definition_key, "function:user.Main");
        assert_eq!(functions[2].file_id, 0);
        assert_eq!(dict.files.as_ref().unwrap().files[0].path, "main.baml");
        assert!(!dict.identity.as_ref().unwrap().fallback_identity);
        assert!(
            function_row_by_id(&dict, FIRST_POOL_FUNCTION_ID).is_some(),
            "id lookup resolves"
        );
        assert_eq!(
            functions[2].def_content_hash.len(),
            32,
            "real rows carry a behavior hash"
        );
    }

    #[test]
    fn unfinalized_program_yields_no_dictionary() {
        let program = Program::default();
        assert!(build_revision_dictionary(&program).is_none());
    }

    #[test]
    fn dict_write_is_idempotent_and_roundtrips() {
        let program = finalized_test_program();
        let dict = build_revision_dictionary(&program).unwrap();
        let dir = std::env::temp_dir().join(format!("baml-dict-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let path = ensure_dict_written(&dir, &dict).unwrap();
        assert!(path.exists());
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("baml_rev_1_")
        );
        let first_bytes = std::fs::read(&path).unwrap();

        // Second write: no-op, same bytes.
        let path2 = ensure_dict_written(&dir, &dict).unwrap();
        assert_eq!(path, path2);
        assert_eq!(std::fs::read(&path).unwrap(), first_bytes);

        let parsed = read_dict(&first_bytes).unwrap();
        assert_eq!(parsed.identity.as_ref().unwrap().revision_id, vec![3u8; 32]);
        assert_eq!(parsed.capture_policy_version, CAPTURE_POLICY_VERSION);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn capture_flags_bitfield_encodes_all_options() {
        let props = FunctionCaptureProps::disabled();
        assert_eq!(capture_flags(props, false), 0);
        let auto_all = FunctionCaptureProps {
            inputs: CaptureOption::Auto,
            output: CaptureOption::Auto,
            error: CaptureOption::Auto,
            promote_on_error: CaptureOption::Auto,
        };
        let flags = capture_flags(auto_all, true);
        assert_eq!(flags & 0b11, 1);
        assert_eq!((flags >> 2) & 0b11, 1);
        assert_eq!((flags >> 4) & 0b11, 1);
        assert_eq!((flags >> 6) & 0b11, 1);
        assert_ne!(flags & (1 << 8), 0, "is_llm");
        assert_ne!(flags & (1 << 9), 0, "captures_any");
        let enabled_output = FunctionCaptureProps::disabled().with_option(
            bex_vm_types::CaptureCategory::Output,
            CaptureOption::Enabled,
        );
        assert_eq!((capture_flags(enabled_output, false) >> 2) & 0b11, 2);
    }
}
