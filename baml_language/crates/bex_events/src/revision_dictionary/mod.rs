//! Versioned, revision-scoped function metadata.
//!
//! This module deliberately owns plain rows rather than compiler/VM objects.
//! The compiler can build the rows in one finalized-program walk, then hand
//! the owned dictionary to the observability consumer without keeping a
//! `Program` alive. Native persistence lives in [`file`].

use std::{collections::HashSet, fmt};

pub use bex_vm_types::{
    ContentIdParseError, FIRST_POOL_FUNCTION_ID, FUNCTION_ID_SPAWN_CLOSURE, FUNCTION_ID_UNKNOWN,
    ProgramIdentity, RevisionId, SourceSnapshotId,
};

#[cfg(not(target_arch = "wasm32"))]
pub mod file;

/// The only `.bamldict` format this module writes.
pub const DICTIONARY_FORMAT_VERSION: u32 = 1;

/// Generated protobuf types for the length-delimited `.bamldict` sections.
#[allow(
    clippy::pedantic,
    clippy::doc_markdown,
    unreachable_pub,
    reason = "prost-generated code"
)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/baml.dictionary.v1.rs"));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub file_id: u32,
    pub path: String,
    pub content_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDictRow {
    pub function_id: u32,
    pub fqn: String,
    pub display_name: String,
    pub declared_name: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub kind: FunctionKind,
    pub origin: FunctionOrigin,
    pub definition_key: String,
    pub owner_type_key: Option<String>,
    pub lambda: Option<LambdaIdentity>,
    pub package_name: Option<String>,
    pub namespace: Vec<String>,
    pub capture_flags: u32,
    pub def_content_hash: [u8; 32],
    pub semantic_lanes: Option<SemanticLanes>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub file_id: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionKind {
    Bytecode,
    SysOp(String),
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    Builtin,
    AutoDerive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaIdentity {
    pub parent_definition_key: String,
    pub ordinal: u32,
    pub kind: LambdaKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LambdaKind {
    Lambda,
    SpawnedClosure,
    Adapter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticLanes {
    pub direct_interface: [u8; 32],
    pub effective_interface: [u8; 32],
    pub direct_implementation: Option<[u8; 32]>,
    pub effective_implementation: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallSiteRow {
    pub call_site_id: u32,
    pub caller_function_id: u32,
    pub callee_function_id: Option<u32>,
    pub file_id: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionDictionary {
    pub identity: ProgramIdentity,
    pub capture_policy_version: u32,
    pub files: Vec<FileRow>,
    pub functions: Vec<FunctionDictRow>,
    pub call_sites: Vec<CallSiteRow>,
}

impl RevisionDictionary {
    /// Build the revision-scoped dictionary in one pure walk over a finalized
    /// program. Source contents are not retained: the compiler-attached
    /// `ProgramSourceFile` rows already carry their BLAKE3 hashes.
    pub fn from_program(
        program: &bex_vm_types::Program,
    ) -> Result<Self, DictionaryValidationError> {
        let identity = program
            .identity
            .clone()
            .ok_or(DictionaryValidationError::MissingProgramIdentity)?;
        let files: Vec<FileRow> = program
            .source_files
            .iter()
            .map(|file| FileRow {
                file_id: file.file_id,
                path: file.project_relative_path.clone(),
                content_hash: file.content_hash,
            })
            .collect();
        let file_ids: HashSet<u32> = files.iter().map(|file| file.file_id).collect();

        let mut functions = Vec::with_capacity(identity.function_count as usize);
        for object in &program.objects {
            let bex_vm_types::Object::Function(function) = object else {
                continue;
            };
            let mut parts = function.name.split('.').collect::<Vec<_>>();
            let display_name = function
                .declared_name
                .clone()
                .or_else(|| parts.last().map(|part| (*part).to_owned()))
                .unwrap_or_else(|| function.name.clone());
            let package_name = (parts.len() > 1).then(|| parts.remove(0).to_owned());
            let namespace = if parts.len() > 1 {
                parts[..parts.len() - 1]
                    .iter()
                    .map(|part| (*part).to_owned())
                    .collect()
            } else {
                Vec::new()
            };
            let span_file_id = function.span.file_id.as_u32();
            let source_span = file_ids.contains(&span_file_id).then(|| SourceSpan {
                file_id: span_file_id,
                span_start: function.span.range.start().into(),
                span_end: function.span.range.end().into(),
                line: function
                    .bytecode
                    .line_table
                    .first()
                    .and_then(|entry| u32::try_from(entry.line).ok())
                    .unwrap_or(0),
            });
            let definition_key = if function.def_meta.definition_key.is_empty() {
                format!("function:{}", function.name)
            } else {
                function.def_meta.definition_key.clone()
            };
            functions.push(FunctionDictRow {
                function_id: function.function_id,
                fqn: function.name.clone(),
                display_name,
                declared_name: function.declared_name.clone(),
                source_span,
                kind: match function.kind {
                    bex_vm_types::FunctionKind::Bytecode => FunctionKind::Bytecode,
                    bex_vm_types::FunctionKind::SysOp(op) => FunctionKind::SysOp(format!("{op:?}")),
                    bex_vm_types::FunctionKind::NativeUnresolved
                    | bex_vm_types::FunctionKind::Native(_) => FunctionKind::Native,
                },
                origin: match function.origin {
                    bex_vm_types::FunctionOrigin::UserDefined => FunctionOrigin::UserDefined,
                    bex_vm_types::FunctionOrigin::Companion => FunctionOrigin::Companion,
                    bex_vm_types::FunctionOrigin::Internal => FunctionOrigin::Internal,
                    bex_vm_types::FunctionOrigin::Builtin => FunctionOrigin::Builtin,
                    bex_vm_types::FunctionOrigin::AutoDerive => FunctionOrigin::AutoDerive,
                },
                definition_key,
                owner_type_key: function.def_meta.owner_type_key.clone(),
                lambda: function
                    .def_meta
                    .lambda
                    .as_ref()
                    .map(|lambda| LambdaIdentity {
                        parent_definition_key: lambda.parent_definition_key.clone(),
                        ordinal: lambda.ordinal,
                        kind: match lambda.kind {
                            bex_vm_types::LambdaKind::Lambda => LambdaKind::Lambda,
                            bex_vm_types::LambdaKind::SpawnedClosure => LambdaKind::SpawnedClosure,
                            bex_vm_types::LambdaKind::Adapter => LambdaKind::Adapter,
                        },
                    }),
                package_name,
                namespace,
                capture_flags: capture_flags(function),
                def_content_hash: definition_content_hash(program, function),
                semantic_lanes: None,
            });
        }

        Self::new(
            identity,
            1,
            files,
            functions,
            collect_call_sites(program, &file_ids),
        )
    }

    /// Builds a canonical dictionary from compiler-owned rows.
    ///
    /// Rows 0 and 1 are reserved and inserted here, while all three tables are
    /// sorted by id. This makes encoding deterministic without requiring
    /// callers to preserve compiler traversal order after building the rows.
    pub fn new(
        identity: ProgramIdentity,
        capture_policy_version: u32,
        mut files: Vec<FileRow>,
        mut functions: Vec<FunctionDictRow>,
        mut call_sites: Vec<CallSiteRow>,
    ) -> Result<Self, DictionaryValidationError> {
        if let Some(row) = functions.iter().find(|row| {
            matches!(
                row.function_id,
                FUNCTION_ID_UNKNOWN | FUNCTION_ID_SPAWN_CLOSURE
            )
        }) {
            return Err(DictionaryValidationError::ReservedFunctionId(
                row.function_id,
            ));
        }

        files.sort_unstable_by_key(|row| row.file_id);
        functions.sort_unstable_by_key(|row| row.function_id);
        call_sites.sort_unstable_by_key(|row| row.call_site_id);

        functions.insert(0, synthetic_spawn_closure_row());
        functions.insert(0, synthetic_unknown_row());

        let dictionary = Self {
            identity,
            capture_policy_version,
            files,
            functions,
            call_sites,
        };
        dictionary.validate()?;
        Ok(dictionary)
    }

    /// Checks the invariants relied on by the native writer and downstream
    /// integer-keyed joins.
    pub fn validate(&self) -> Result<(), DictionaryValidationError> {
        if self.functions.first().map(|row| row.function_id) != Some(FUNCTION_ID_UNKNOWN)
            || self.functions.get(1).map(|row| row.function_id) != Some(FUNCTION_ID_SPAWN_CLOSURE)
        {
            return Err(DictionaryValidationError::MissingSyntheticFunctions);
        }

        let expected_rows = usize::try_from(self.identity.function_count)
            .unwrap_or(usize::MAX)
            .saturating_add(2);
        if self.functions.len() != expected_rows {
            return Err(DictionaryValidationError::FunctionCount {
                header: self.identity.function_count,
                rows: self.functions.len().saturating_sub(2),
            });
        }

        let mut file_ids = HashSet::with_capacity(self.files.len());
        for file in &self.files {
            if !file_ids.insert(file.file_id) {
                return Err(DictionaryValidationError::DuplicateFileId(file.file_id));
            }
        }

        let mut function_ids = HashSet::with_capacity(self.functions.len());
        let mut definition_keys = HashSet::with_capacity(self.functions.len());
        for (index, function) in self.functions.iter().enumerate() {
            if !function_ids.insert(function.function_id) {
                return Err(DictionaryValidationError::DuplicateFunctionId(
                    function.function_id,
                ));
            }
            if !definition_keys.insert(function.definition_key.as_str()) {
                return Err(DictionaryValidationError::DuplicateDefinitionKey(
                    function.definition_key.clone(),
                ));
            }
            if index >= 2 {
                let expected = FIRST_POOL_FUNCTION_ID
                    .saturating_add(u32::try_from(index - 2).unwrap_or(u32::MAX));
                if function.function_id != expected {
                    return Err(DictionaryValidationError::NonDenseFunctionId {
                        expected,
                        actual: function.function_id,
                    });
                }
            }
            if let Some(span) = function.source_span {
                validate_span(span.file_id, span.span_start, span.span_end, &file_ids)?;
            }
        }

        let mut call_site_ids = HashSet::with_capacity(self.call_sites.len());
        for call_site in &self.call_sites {
            if !call_site_ids.insert(call_site.call_site_id) {
                return Err(DictionaryValidationError::DuplicateCallSiteId(
                    call_site.call_site_id,
                ));
            }
            if !function_ids.contains(&call_site.caller_function_id) {
                return Err(DictionaryValidationError::UnknownFunctionId(
                    call_site.caller_function_id,
                ));
            }
            if let Some(callee) = call_site.callee_function_id {
                if !function_ids.contains(&callee) {
                    return Err(DictionaryValidationError::UnknownFunctionId(callee));
                }
            }
            validate_span(
                call_site.file_id,
                call_site.span_start,
                call_site.span_end,
                &file_ids,
            )?;
        }

        Ok(())
    }
}

fn capture_option_bits(option: bex_vm_types::CaptureOption) -> u32 {
    match option {
        bex_vm_types::CaptureOption::Disabled => 0,
        bex_vm_types::CaptureOption::Auto => 1,
        bex_vm_types::CaptureOption::Enabled => 2,
    }
}

fn capture_flags(function: &bex_vm_types::Function) -> u32 {
    let capture = function.capture;
    let mut flags = capture_option_bits(capture.inputs)
        | (capture_option_bits(capture.output) << 2)
        | (capture_option_bits(capture.error) << 4)
        | (capture_option_bits(capture.promote_on_error) << 6);
    if function.body_meta.is_some() {
        flags |= 1 << 8;
    }
    if flags & 0xff != 0 {
        flags |= 1 << 9;
    }
    flags
}

fn hash_borsh<T: borsh::BorshSerialize>(hasher: &mut blake3::Hasher, value: &T) {
    let bytes = borsh::to_vec(value).expect("in-memory borsh serialization cannot fail");
    hasher.update(
        &u64::try_from(bytes.len())
            .expect("borsh projection exceeds u64")
            .to_le_bytes(),
    );
    hasher.update(&bytes);
}

fn referent_definition_key(
    program: &bex_vm_types::Program,
    index: bex_vm_types::ObjectIndex,
) -> String {
    match program.objects.get(index.raw()) {
        Some(bex_vm_types::Object::Function(function)) => {
            if function.def_meta.definition_key.is_empty() {
                format!("function:{}", function.name)
            } else {
                function.def_meta.definition_key.clone()
            }
        }
        Some(bex_vm_types::Object::Class(class)) => format!("class:{}", class.name),
        Some(bex_vm_types::Object::Enum(enumeration)) => format!("enum:{}", enumeration.name),
        Some(object) => {
            let bytes = borsh::to_vec(object).unwrap_or_default();
            format!("object:{}", blake3::hash(&bytes).to_hex())
        }
        None => "object:<invalid>".to_owned(),
    }
}

fn definition_content_hash(
    program: &bex_vm_types::Program,
    function: &bex_vm_types::Function,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"baml.def.v1\0");
    hasher.update(&[match function.kind {
        bex_vm_types::FunctionKind::Bytecode => 0,
        bex_vm_types::FunctionKind::SysOp(_) => 1,
        bex_vm_types::FunctionKind::NativeUnresolved | bex_vm_types::FunctionKind::Native(_) => 2,
    }]);
    hasher.update(
        &u32::try_from(function.arity)
            .expect("function arity exceeds u32")
            .to_le_bytes(),
    );
    hash_borsh(&mut hasher, &function.param_types);
    hash_borsh(&mut hasher, &function.return_type);
    hash_borsh(&mut hasher, &function.throws_type);
    hash_borsh(&mut hasher, &function.bytecode.instructions);
    for constant in &function.bytecode.constants {
        match constant {
            bex_vm_types::ConstValue::Object(index) => {
                hasher.update(&[5]);
                let key = referent_definition_key(program, *index);
                hasher.update(
                    &u32::try_from(key.len())
                        .expect("definition key exceeds u32")
                        .to_le_bytes(),
                );
                hasher.update(key.as_bytes());
            }
            other => hash_borsh(&mut hasher, other),
        }
    }
    for table in &function.bytecode.jump_tables {
        hash_borsh(&mut hasher, &table.min);
        hash_borsh(&mut hasher, &table.offsets);
        hash_borsh(&mut hasher, &table.default);
    }
    hash_borsh(&mut hasher, &function.bytecode.field_copy_sets);
    hash_borsh(&mut hasher, &function.bytecode.class_init_plans);
    for table in &function.bytecode.match_hash_tables {
        hash_borsh(&mut hasher, &table.multiply);
        hash_borsh(&mut hasher, &table.shift);
        hash_borsh(&mut hasher, &table.mask);
        hash_borsh(&mut hasher, &table.entries);
    }
    hash_borsh(&mut hasher, &function.bytecode.exception_table);
    hash_borsh(&mut hasher, &function.bytecode.handler_context_table);
    *hasher.finalize().as_bytes()
}

fn direct_callee(
    program: &bex_vm_types::Program,
    global: bex_vm_types::GlobalIndex,
) -> Option<u32> {
    let bex_vm_types::ConstValue::Object(index) = program.globals.get(global.raw())? else {
        return None;
    };
    let bex_vm_types::Object::Function(function) = program.objects.get(index.raw())? else {
        return None;
    };
    Some(function.function_id)
}

fn collect_call_sites(
    program: &bex_vm_types::Program,
    file_ids: &HashSet<u32>,
) -> Vec<CallSiteRow> {
    let mut rows = Vec::new();
    for object in &program.objects {
        let bex_vm_types::Object::Function(function) = object else {
            continue;
        };
        for (pc, instruction) in function.bytecode.instructions.iter().enumerate() {
            let (is_call, callee) = match instruction {
                bex_vm_types::Instruction::Call { callee, .. }
                | bex_vm_types::Instruction::CallWithRuntimeId { callee, .. }
                | bex_vm_types::Instruction::SysOp(callee)
                | bex_vm_types::Instruction::SysOpWithRuntimeId(callee) => {
                    (true, direct_callee(program, *callee))
                }
                bex_vm_types::Instruction::CallIndirect
                | bex_vm_types::Instruction::CallIndirectWithRuntimeId
                | bex_vm_types::Instruction::VirtualCall { .. }
                | bex_vm_types::Instruction::VirtualCallWithRuntimeId { .. } => (true, None),
                _ => (false, None),
            };
            if !is_call {
                continue;
            }
            let Some(line) = function
                .bytecode
                .line_table
                .iter()
                .rev()
                .find(|entry| entry.pc <= pc)
            else {
                continue;
            };
            let file_id = line.span.file_id.as_u32();
            if !file_ids.contains(&file_id) {
                continue;
            }
            rows.push(CallSiteRow {
                call_site_id: u32::try_from(rows.len() + 1).unwrap_or(u32::MAX),
                caller_function_id: function.function_id,
                callee_function_id: callee,
                file_id,
                span_start: line.span.range.start().into(),
                span_end: line.span.range.end().into(),
                line: u32::try_from(line.line).unwrap_or(u32::MAX),
            });
        }
    }
    rows
}

fn validate_span(
    file_id: u32,
    start: u32,
    end: u32,
    file_ids: &HashSet<u32>,
) -> Result<(), DictionaryValidationError> {
    if !file_ids.contains(&file_id) {
        return Err(DictionaryValidationError::UnknownFileId(file_id));
    }
    if start > end {
        return Err(DictionaryValidationError::InvalidSpan { start, end });
    }
    Ok(())
}

fn synthetic_unknown_row() -> FunctionDictRow {
    synthetic_row(
        FUNCTION_ID_UNKNOWN,
        "baml.<unknown-function>",
        "<unknown function>",
        "synthetic:unknown-function",
    )
}

fn synthetic_spawn_closure_row() -> FunctionDictRow {
    synthetic_row(
        FUNCTION_ID_SPAWN_CLOSURE,
        "baml.<spawn-closure>",
        "<spawn closure>",
        "synthetic:spawn-closure",
    )
}

fn synthetic_row(
    function_id: u32,
    fqn: &str,
    display_name: &str,
    definition_key: &str,
) -> FunctionDictRow {
    FunctionDictRow {
        function_id,
        fqn: fqn.to_owned(),
        display_name: display_name.to_owned(),
        declared_name: None,
        source_span: None,
        kind: FunctionKind::Native,
        origin: FunctionOrigin::Internal,
        definition_key: definition_key.to_owned(),
        owner_type_key: None,
        lambda: None,
        package_name: None,
        namespace: Vec::new(),
        capture_flags: 0,
        // Synthetic frames have no compiler definition body to hash.
        def_content_hash: [0; 32],
        semantic_lanes: None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DictionaryValidationError {
    MissingProgramIdentity,
    ReservedFunctionId(u32),
    MissingSyntheticFunctions,
    FunctionCount { header: u32, rows: usize },
    DuplicateFileId(u32),
    DuplicateFunctionId(u32),
    DuplicateDefinitionKey(String),
    NonDenseFunctionId { expected: u32, actual: u32 },
    DuplicateCallSiteId(u32),
    UnknownFileId(u32),
    UnknownFunctionId(u32),
    InvalidSpan { start: u32, end: u32 },
}

impl fmt::Display for DictionaryValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProgramIdentity => {
                formatter.write_str("program is missing finalized source/revision identity")
            }
            Self::ReservedFunctionId(id) => {
                write!(
                    formatter,
                    "function id {id} is reserved for a synthetic row"
                )
            }
            Self::MissingSyntheticFunctions => {
                formatter.write_str("dictionary must begin with synthetic function rows 0 and 1")
            }
            Self::FunctionCount { header, rows } => write!(
                formatter,
                "function count mismatch: identity says {header}, dictionary has {rows} compiler rows"
            ),
            Self::DuplicateFileId(id) => write!(formatter, "duplicate file id {id}"),
            Self::DuplicateFunctionId(id) => write!(formatter, "duplicate function id {id}"),
            Self::DuplicateDefinitionKey(key) => {
                write!(formatter, "duplicate definition key `{key}`")
            }
            Self::NonDenseFunctionId { expected, actual } => write!(
                formatter,
                "non-dense compiler function id: expected {expected}, got {actual}"
            ),
            Self::DuplicateCallSiteId(id) => write!(formatter, "duplicate call-site id {id}"),
            Self::UnknownFileId(id) => write!(formatter, "row references unknown file id {id}"),
            Self::UnknownFunctionId(id) => {
                write!(formatter, "row references unknown function id {id}")
            }
            Self::InvalidSpan { start, end } => {
                write!(formatter, "invalid source span {start}..{end}")
            }
        }
    }
}

impl std::error::Error for DictionaryValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_vm_types::{
        Bytecode, DefinitionMeta, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin,
        Object, Program,
    };

    fn function(name: &str) -> Object {
        Object::Function(Box::new(Function {
            name: format!("pkg.{name}"),
            source_file: "main.baml".to_owned(),
            docstring: Some("debug-only documentation".to_owned()),
            declared_name: Some(name.to_owned()),
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode::default(),
            kind: FunctionKind::Bytecode,
            local_names: vec!["debug_local".to_owned()],
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
            display_return_type: "never".to_owned(),
            throws_type: baml_type::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            def_meta: DefinitionMeta {
                definition_key: format!("function:pkg.{name}"),
                ..DefinitionMeta::default()
            },
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
        }))
    }

    fn program() -> Program {
        let mut program = Program::new();
        program.objects.push(function("first"));
        program.objects.push(function("second"));
        bex_vm_types::finalize_legacy_program_identity(&mut program, "test-compiler");
        program
    }

    #[test]
    fn finalized_program_builds_dense_dictionary_with_synthetic_rows() {
        let dictionary = RevisionDictionary::from_program(&program()).unwrap();
        assert_eq!(dictionary.functions.len(), 4);
        assert_eq!(
            dictionary
                .functions
                .iter()
                .map(|row| row.function_id)
                .collect::<Vec<_>>(),
            vec![0, 1, 16, 17]
        );
        assert_eq!(dictionary.functions[2].definition_key, "function:pkg.first");
        assert_ne!(dictionary.functions[2].def_content_hash, [0; 32]);
    }

    #[test]
    fn unrelated_definition_edit_does_not_churn_other_content_hashes() {
        let before_program = program();
        let before = RevisionDictionary::from_program(&before_program).unwrap();
        let mut after_program = before_program;
        let Object::Function(second) = after_program.objects.get_mut(1).unwrap() else {
            unreachable!()
        };
        second
            .bytecode
            .constants
            .push(bex_vm_types::ConstValue::Int(42));
        let after = RevisionDictionary::from_program(&after_program).unwrap();

        assert_eq!(
            before.functions[2].def_content_hash, after.functions[2].def_content_hash,
            "editing a different definition must preserve the first definition hash"
        );
        assert_ne!(
            before.functions[3].def_content_hash,
            after.functions[3].def_content_hash
        );
    }
}
