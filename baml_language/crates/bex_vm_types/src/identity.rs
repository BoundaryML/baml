//! Derived identities for executable [`Program`](crate::Program)s.
//!
//! Function ids deliberately do not participate in the serialized program
//! format: a compilation unit cannot know its final object-pool position, and
//! the final pool order is already the linker's deterministic layout contract.
//! Every runnable program is therefore finalized with [`assign_function_ids`]
//! after its final layout is known (and again after deserialization).

use std::{fmt, str::FromStr};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::{Object, Program};

/// Unattributable or runtime-synthesized function identity.
pub const FUNCTION_ID_UNKNOWN: u32 = 0;

/// Synthetic identity used for spawn-closure child roots.
pub const FUNCTION_ID_SPAWN_CLOSURE: u32 = 1;

/// First id available to a real `Object::Function` in the final object pool.
///
/// The low range is kept for runtime/host frames which do not have a pool
/// object. Besides ids 0 and 1 above, ids 2 through 15 are currently reserved.
pub const FIRST_POOL_FUNCTION_ID: u32 = 16;

const REVISION_PREFIX: &str = "baml_rev_1_";
const SOURCE_PREFIX: &str = "baml_src_1_";
const HASH_BYTES: usize = 32;

macro_rules! content_id {
    ($name:ident, $prefix:ident) => {
        #[derive(
            Clone, Copy, Debug, Default, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize,
        )]
        pub struct $name(pub [u8; HASH_BYTES]);

        impl $name {
            #[must_use]
            pub const fn from_bytes(bytes: [u8; HASH_BYTES]) -> Self {
                Self(bytes)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; HASH_BYTES] {
                &self.0
            }

            #[must_use]
            pub const fn into_bytes(self) -> [u8; HASH_BYTES] {
                self.0
            }

            #[must_use]
            pub fn to_wire_string(self) -> String {
                let mut encoded = String::with_capacity($prefix.len() + 43);
                encoded.push_str($prefix);
                URL_SAFE_NO_PAD.encode_string(self.0, &mut encoded);
                encoded
            }

            pub fn from_wire_str(value: &str) -> Result<Self, ContentIdParseError> {
                let payload = value
                    .strip_prefix($prefix)
                    .ok_or(ContentIdParseError::InvalidPrefix { expected: $prefix })?;
                let decoded = URL_SAFE_NO_PAD
                    .decode(payload)
                    .map_err(|_| ContentIdParseError::InvalidBase64)?;
                let actual = decoded.len();
                let bytes = decoded.try_into().map_err(|_: Vec<u8>| {
                    ContentIdParseError::InvalidLength {
                        expected: HASH_BYTES,
                        actual,
                    }
                })?;
                Ok(Self(bytes))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.to_wire_string())
            }
        }

        impl FromStr for $name {
            type Err = ContentIdParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::from_wire_str(value)
            }
        }
    };
}

content_id!(RevisionId, REVISION_PREFIX);
content_id!(SourceSnapshotId, SOURCE_PREFIX);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentIdParseError {
    InvalidPrefix { expected: &'static str },
    InvalidBase64,
    InvalidLength { expected: usize, actual: usize },
}

impl fmt::Display for ContentIdParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrefix { expected } => {
                write!(
                    formatter,
                    "invalid content-id prefix; expected `{expected}`"
                )
            }
            Self::InvalidBase64 => formatter.write_str("invalid content-id base64 payload"),
            Self::InvalidLength { expected, actual } => write!(
                formatter,
                "invalid content-id byte length: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for ContentIdParseError {}

/// Source/toolchain identity attached to a runnable program.
///
/// `Program` skips this in its own borsh representation. Enclosing containers
/// such as `PackEnvelope` carry it alongside the program and restore it at
/// their load seam.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProgramIdentity {
    pub revision_id: RevisionId,
    pub source_snapshot_id: SourceSnapshotId,
    pub compiler_id: String,
    /// Real object-pool functions; reserved ids 0..=15 are excluded.
    pub function_count: u32,
}

/// A runnable program reached the engine without the compile/load identity
/// invariant being established.
///
/// The engine only verifies this invariant. Compiler/linker and deserialization
/// seams own all mutation through [`finalize_program_identity`],
/// [`finalize_legacy_program_identity`], or [`assign_function_ids`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProgramIdentityError {
    #[error("program identity is missing")]
    MissingIdentity,
    #[error(
        "program identity function count mismatch: identity={identity_count}, pool={pool_count}"
    )]
    FunctionCountMismatch {
        identity_count: u32,
        pool_count: u32,
    },
    #[error(
        "program function id mismatch at object {object_index}: expected {expected}, found {actual}"
    )]
    FunctionIdMismatch {
        object_index: usize,
        expected: u32,
        actual: u32,
    },
}

/// One user source file participating in source-snapshot identity.
#[derive(Clone, Copy, Debug)]
pub struct SourceIdentityInput<'a> {
    /// Compiler file id used by source spans. It is metadata only and does not
    /// participate in the snapshot hash.
    pub file_id: u32,
    pub project_relative_path: &'a str,
    pub content: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProgramSourceFile {
    pub file_id: u32,
    pub project_relative_path: String,
    pub content_hash: [u8; 32],
}

/// Exact compile options participating in revision identity.
#[derive(Clone, Copy, Debug)]
pub struct RevisionOptions<'a> {
    pub compiler_id: &'a str,
    pub opt_level: u8,
    pub emit_test_cases: bool,
}

/// Hash user sources according to the canonical `baml.snapshot.v1` framing.
#[must_use]
pub fn compute_source_snapshot_id(
    files: &[SourceIdentityInput<'_>],
    manifest: Option<&[u8]>,
) -> SourceSnapshotId {
    let mut sorted = files.to_vec();
    sorted.sort_unstable_by(|left, right| {
        left.project_relative_path
            .as_bytes()
            .cmp(right.project_relative_path.as_bytes())
    });

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"baml.snapshot.v1\0");
    hasher.update(
        &u64::try_from(sorted.len())
            .expect("source file count exceeds u64")
            .to_le_bytes(),
    );
    for file in sorted {
        let path = file.project_relative_path.as_bytes();
        hasher.update(
            &u32::try_from(path.len())
                .expect("project-relative source path exceeds u32")
                .to_le_bytes(),
        );
        hasher.update(path);
        hasher.update(blake3::hash(file.content).as_bytes());
    }
    if let Some(manifest) = manifest {
        hasher.update(&[1]);
        hasher.update(blake3::hash(manifest).as_bytes());
    } else {
        hasher.update(&[0]);
    }
    SourceSnapshotId(*hasher.finalize().as_bytes())
}

/// Hash `source × toolchain × options` with canonical revision framing.
#[must_use]
pub fn compute_revision_id(
    source_snapshot_id: SourceSnapshotId,
    options: RevisionOptions<'_>,
) -> RevisionId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"baml.revision.v1\0");
    hasher.update(source_snapshot_id.as_bytes());
    hasher.update(
        &u32::try_from(options.compiler_id.len())
            .expect("compiler id exceeds u32")
            .to_le_bytes(),
    );
    hasher.update(options.compiler_id.as_bytes());
    hasher.update(&[options.opt_level]);
    hasher.update(&[u8::from(options.emit_test_cases)]);
    hasher.update(&0_u16.to_le_bytes());
    RevisionId(*hasher.finalize().as_bytes())
}

/// Stamp dense function ids and attach canonical source/revision identity.
pub fn finalize_program_identity(
    program: &mut Program,
    files: &[SourceIdentityInput<'_>],
    manifest: Option<&[u8]>,
    options: RevisionOptions<'_>,
) -> ProgramIdentity {
    let function_count = assign_function_ids(program);
    let source_snapshot_id = compute_source_snapshot_id(files, manifest);
    let revision_id = compute_revision_id(source_snapshot_id, options);
    let identity = ProgramIdentity {
        revision_id,
        source_snapshot_id,
        compiler_id: options.compiler_id.to_owned(),
        function_count,
    };
    program.identity = Some(identity.clone());
    program.source_files = files
        .iter()
        .map(|file| ProgramSourceFile {
            file_id: file.file_id,
            project_relative_path: file.project_relative_path.to_owned(),
            content_hash: *blake3::hash(file.content).as_bytes(),
        })
        .collect();
    program.source_files.sort_unstable_by(|left, right| {
        left.project_relative_path
            .as_bytes()
            .cmp(right.project_relative_path.as_bytes())
    });
    identity
}

/// Attach deterministic identity to an identity-less legacy program.
///
/// This path serializes the program once at load time and is never used by the
/// compiler's normal source-based finalizer.
pub fn finalize_legacy_program_identity(
    program: &mut Program,
    compiler_id: &str,
) -> ProgramIdentity {
    let bytes = borsh::to_vec(program).expect("serializing a decoded Program cannot fail");

    let mut source_hasher = blake3::Hasher::new();
    source_hasher.update(b"baml.snapshot.fallback.v1\0");
    source_hasher.update(&bytes);
    let source_snapshot_id = SourceSnapshotId(*source_hasher.finalize().as_bytes());

    let mut revision_hasher = blake3::Hasher::new();
    revision_hasher.update(b"baml.revision.fallback.v1\0");
    revision_hasher.update(&bytes);
    let revision_id = RevisionId(*revision_hasher.finalize().as_bytes());

    let function_count = assign_function_ids(program);
    let identity = ProgramIdentity {
        revision_id,
        source_snapshot_id,
        compiler_id: compiler_id.to_owned(),
        function_count,
    };
    program.identity = Some(identity.clone());
    identity
}

/// Stamp dense ids onto every [`Object::Function`] in final pool order.
///
/// This is derived state and is intentionally overwritten rather than
/// conditionally filled, making the operation idempotent and repairing any
/// stale ids after relinking. The returned value is the number of real pool
/// functions (synthetic reserved ids are not included).
///
/// # Panics
///
/// Panics if a program contains enough functions to exhaust the `u32` identity
/// space. Such a program cannot fit in a practical address space, but checking
/// keeps wraparound from silently violating uniqueness.
pub fn assign_function_ids(program: &mut Program) -> u32 {
    let mut function_count = 0_u32;

    for object in &mut program.objects {
        let Object::Function(function) = object else {
            continue;
        };

        function.function_id = FIRST_POOL_FUNCTION_ID
            .checked_add(function_count)
            .expect("program has more functions than the u32 identity space supports");
        function_count = function_count
            .checked_add(1)
            .expect("program has more functions than the u32 identity space supports");
    }

    function_count
}

/// Verify the complete runnable-program identity invariant without mutation.
///
/// This is intentionally stricter than comparing `function_count`: stale or
/// zero ids can otherwise survive with the correct number of pool functions
/// and corrupt profiling attribution. Real function ids must be dense in final
/// object-pool order starting at [`FIRST_POOL_FUNCTION_ID`].
pub fn verify_program_identity(
    program: &Program,
) -> Result<&ProgramIdentity, ProgramIdentityError> {
    let identity = program
        .identity
        .as_ref()
        .ok_or(ProgramIdentityError::MissingIdentity)?;
    let mut pool_count = 0_u32;
    for (object_index, object) in program.objects.iter().enumerate() {
        let Object::Function(function) = object else {
            continue;
        };
        let expected = FIRST_POOL_FUNCTION_ID
            .checked_add(pool_count)
            .expect("program has more functions than the u32 identity space supports");
        if function.function_id != expected {
            return Err(ProgramIdentityError::FunctionIdMismatch {
                object_index,
                expected,
                actual: function.function_id,
            });
        }
        pool_count = pool_count
            .checked_add(1)
            .expect("program has more functions than the u32 identity space supports");
    }
    if identity.function_count != pool_count {
        return Err(ProgramIdentityError::FunctionCountMismatch {
            identity_count: identity.function_count,
            pool_count,
        });
    }
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use crate::{
        Bytecode, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin, Object, Program,
    };

    use super::{
        FIRST_POOL_FUNCTION_ID, FUNCTION_ID_SPAWN_CLOSURE, FUNCTION_ID_UNKNOWN, RevisionId,
        RevisionOptions, SourceIdentityInput, SourceSnapshotId, assign_function_ids,
        compute_revision_id, compute_source_snapshot_id, finalize_legacy_program_identity,
        verify_program_identity,
    };

    fn function(name: &str, stale_id: u32) -> Object {
        Object::Function(Box::new(Function {
            name: name.to_string(),
            source_file: "test.baml".to_string(),
            docstring: None,
            declared_name: Some(name.to_string()),
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode::default(),
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: baml_type::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "never".to_string(),
            throws_type: baml_type::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            def_meta: crate::DefinitionMeta::default(),
            capture: FunctionCaptureProps::disabled(),
            function_id: stale_id,
        }))
    }

    fn function_ids(program: &Program) -> Vec<u32> {
        program
            .objects
            .iter()
            .filter_map(|object| match object {
                Object::Function(function) => Some(function.function_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn assigns_dense_ids_in_pool_order_and_skips_non_functions() {
        let mut program = Program::new();
        program.objects.push(function("first", 99));
        program
            .objects
            .push(Object::String("not a function".into()));
        program.objects.push(function("second", 7));

        assert_eq!(assign_function_ids(&mut program), 2);
        assert_eq!(
            function_ids(&program),
            vec![FIRST_POOL_FUNCTION_ID, FIRST_POOL_FUNCTION_ID + 1]
        );
        assert_eq!(FUNCTION_ID_UNKNOWN, 0);
        assert_eq!(FUNCTION_ID_SPAWN_CLOSURE, 1);
    }

    #[test]
    fn assignment_is_idempotent_and_repairs_stale_ids() {
        let mut program = Program::new();
        program.objects.push(function("first", 0));
        program.objects.push(function("second", u32::MAX));

        let count = assign_function_ids(&mut program);
        let once = function_ids(&program);
        assert_eq!(assign_function_ids(&mut program), count);
        assert_eq!(function_ids(&program), once);

        let Object::Function(first) = program.objects.first_mut().expect("first function") else {
            unreachable!()
        };
        first.function_id = 3;
        assert_eq!(assign_function_ids(&mut program), count);
        assert_eq!(function_ids(&program), once);
    }

    #[test]
    fn serialization_omits_derived_ids() {
        let mut program = Program::new();
        program.objects.push(function("only", 0));
        let before = borsh::to_vec(&program).expect("serialize unassigned program");

        assign_function_ids(&mut program);
        let after = borsh::to_vec(&program).expect("serialize assigned program");

        assert_eq!(before, after, "function_id must remain #[borsh(skip)]");
        let decoded: Program = borsh::from_slice(&after).expect("deserialize program");
        assert_eq!(function_ids(&decoded), vec![FUNCTION_ID_UNKNOWN]);
    }

    #[test]
    fn source_snapshot_is_order_independent_but_path_and_manifest_sensitive() {
        let a = SourceIdentityInput {
            file_id: 17,
            project_relative_path: "a.baml",
            content: b"function A() -> string { \"a\" }",
        };
        let b = SourceIdentityInput {
            file_id: 18,
            project_relative_path: "nested/b.baml",
            content: b"function B() -> string { \"b\" }",
        };
        let left = compute_source_snapshot_id(&[a, b], None);
        assert_eq!(left, compute_source_snapshot_id(&[b, a], None));
        assert_ne!(left, compute_source_snapshot_id(&[a, b], Some(b"")));
        assert_ne!(
            left,
            compute_source_snapshot_id(
                &[
                    SourceIdentityInput {
                        project_relative_path: "renamed.baml",
                        ..a
                    },
                    b,
                ],
                None,
            )
        );
    }

    #[test]
    fn revision_changes_with_each_compile_option() {
        let source = SourceSnapshotId([7; 32]);
        let base = compute_revision_id(
            source,
            RevisionOptions {
                compiler_id: "compiler-a",
                opt_level: 1,
                emit_test_cases: false,
            },
        );
        for options in [
            RevisionOptions {
                compiler_id: "compiler-b",
                opt_level: 1,
                emit_test_cases: false,
            },
            RevisionOptions {
                compiler_id: "compiler-a",
                opt_level: 2,
                emit_test_cases: false,
            },
            RevisionOptions {
                compiler_id: "compiler-a",
                opt_level: 1,
                emit_test_cases: true,
            },
        ] {
            assert_ne!(base, compute_revision_id(source, options));
        }
    }

    #[test]
    fn content_ids_have_strict_prefixed_roundtrip() {
        let revision = RevisionId([0xA5; 32]);
        let source = SourceSnapshotId([0x5A; 32]);
        assert_eq!(
            revision.to_string().parse::<RevisionId>().unwrap(),
            revision
        );
        assert_eq!(
            source.to_string().parse::<SourceSnapshotId>().unwrap(),
            source
        );
        assert!(revision.to_string().parse::<SourceSnapshotId>().is_err());
    }

    #[test]
    fn legacy_fallback_is_stable_and_attached() {
        let mut first = Program::new();
        first.objects.push(function("only", 0));
        let mut second = first.clone();
        let left = finalize_legacy_program_identity(&mut first, "legacy");
        let right = finalize_legacy_program_identity(&mut second, "legacy");
        assert_eq!(left, right);
        assert_eq!(first.identity, Some(left));
        assert_eq!(
            verify_program_identity(&first),
            Ok(first.identity.as_ref().unwrap())
        );
    }

    #[test]
    fn verification_rejects_missing_stale_and_miscounted_identity_without_repair() {
        let mut program = Program::new();
        program.objects.push(function("first", 0));
        assert_eq!(
            verify_program_identity(&program),
            Err(super::ProgramIdentityError::MissingIdentity)
        );

        finalize_legacy_program_identity(&mut program, "legacy");
        let expected_identity = program.identity.clone();
        let Object::Function(first) = program.objects.first_mut().unwrap() else {
            unreachable!()
        };
        first.function_id = 99;
        assert_eq!(
            verify_program_identity(&program),
            Err(super::ProgramIdentityError::FunctionIdMismatch {
                object_index: 0,
                expected: FIRST_POOL_FUNCTION_ID,
                actual: 99,
            })
        );
        assert_eq!(
            program.identity, expected_identity,
            "verification is read-only"
        );

        let Object::Function(first) = program.objects.first_mut().unwrap() else {
            unreachable!()
        };
        first.function_id = FIRST_POOL_FUNCTION_ID;
        program.identity.as_mut().unwrap().function_count = 2;
        assert_eq!(
            verify_program_identity(&program),
            Err(super::ProgramIdentityError::FunctionCountMismatch {
                identity_count: 2,
                pool_count: 1,
            })
        );
    }
}
