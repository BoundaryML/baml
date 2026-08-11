//! Compile-time program identity (observability design §4).
//!
//! Function identity is a dense `u32` assigned by the compiler in final
//! object-pool order; revision identity is *source × toolchain × options*,
//! hashed with BLAKE3-256. The VM hot path never interns or passes strings:
//! everything here is fixed before the program runs, and names resolve
//! through the per-revision dictionary written once per revision.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::{Function, Object, Program};

/// Reserved id for unattributable frames (trampolines, host-closure shims,
/// unresolvable callees). Pre-existing sentinel, kept: records carrying `0`
/// mean "no compiler-known function".
pub const FUNCTION_ID_UNKNOWN: u32 = 0;

/// Reserved id for spawn-closure child roots whose closure has no
/// compiler-known identity of its own.
pub const FUNCTION_ID_SPAWN_CLOSURE: u32 = 1;

// Ids 2..=15 are reserved for future synthetic rows (host-call frame, GC
// frame, native frame, ...). The dictionary always emits rows for the
// reserved range first, so readers can resolve them without special cases.

/// First id handed to a real pool function.
pub const FIRST_POOL_FUNCTION_ID: u32 = 16;

/// Canonical FQN for the [`FUNCTION_ID_UNKNOWN`] dictionary row (kept from
/// the engine's interim provider so existing readers keep resolving it).
pub const UNKNOWN_FUNCTION_FQN: &str = "baml.<unknown-function>";
/// Display name for the unknown-function row.
pub const UNKNOWN_FUNCTION_DISPLAY_NAME: &str = "<unknown-function>";
/// Canonical FQN for the [`FUNCTION_ID_SPAWN_CLOSURE`] dictionary row.
pub const SPAWN_CLOSURE_FQN: &str = "baml.<spawn-closure>";
/// Display name for the spawn-closure row.
pub const SPAWN_CLOSURE_DISPLAY_NAME: &str = "<spawn-closure>";

/// Stamp dense ids onto every [`Object::Function`] in pool order.
///
/// Pool order is the rule because it is deterministic (the linker's layout
/// contract, pinned by the B-693 byte-identity oracles) and it is exactly
/// the walk the engine performed when it was the interim id provider. The
/// VM reads `f.function_id` off the heap object at call time — no VM change.
///
/// Idempotent: ids are derived state; re-running produces the same ids.
/// Returns the number of functions stamped.
pub fn assign_function_ids(program: &mut Program) -> u32 {
    let mut next = FIRST_POOL_FUNCTION_ID;
    for obj in program.objects.iter_mut() {
        if let Object::Function(func) = obj {
            func.function_id = next;
            next += 1;
        }
    }
    next - FIRST_POOL_FUNCTION_ID
}

/// Debug-check that every pool function carries the id [`assign_function_ids`]
/// would give it. Release builds check only the last function (cheap tail
/// probe); debug builds check every row. Used by the engine walk, which is
/// verify-only now that assignment happens at compile time.
pub fn verify_function_ids(program: &Program) -> bool {
    let mut next = FIRST_POOL_FUNCTION_ID;
    let mut last_ok = true;
    for obj in program.objects.iter() {
        if let Object::Function(func) = obj {
            let ok = func.function_id == next;
            debug_assert!(
                ok,
                "function id {} != expected {next} for {:?} — program not finalized \
                 (finalize_program_identity must run at every Program materialization site)",
                func.function_id, func.name
            );
            last_ok = ok;
            next += 1;
        }
    }
    last_ok
}

/// BLAKE3-256 of *source × nothing else* (design §4.3): what the user means
/// by "the same code".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct SourceSnapshotId(pub [u8; 32]);

/// BLAKE3-256 of *source × toolchain × options* (design §4.3): the unit the
/// revision dictionary is keyed by, and the scope in which `function_id`
/// means anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RevisionId(pub [u8; 32]);

const REVISION_ID_PREFIX: &str = "baml_rev_1_";
const SOURCE_SNAPSHOT_ID_PREFIX: &str = "baml_src_1_";

fn encode_b64(prefix: &str, bytes: &[u8; 32]) -> String {
    use base64::Engine as _;
    format!(
        "{prefix}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

fn decode_b64(prefix: &str, value: &str) -> Option<[u8; 32]> {
    use base64::Engine as _;
    let rest = value.strip_prefix(prefix)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(rest.as_bytes())
        .ok()?;
    decoded.try_into().ok()
}

impl SourceSnapshotId {
    /// `baml_src_1_` + base64url(32 B) — the public string form (header
    /// field 10, dictionaries, exports).
    #[must_use]
    pub fn encode(&self) -> String {
        encode_b64(SOURCE_SNAPSHOT_ID_PREFIX, &self.0)
    }

    #[must_use]
    pub fn decode(value: &str) -> Option<Self> {
        decode_b64(SOURCE_SNAPSHOT_ID_PREFIX, value).map(Self)
    }
}

impl RevisionId {
    /// `baml_rev_1_` + base64url(32 B) — the public string form (header
    /// field 11, `.bamldict` file names, cross-revision joins).
    #[must_use]
    pub fn encode(&self) -> String {
        encode_b64(REVISION_ID_PREFIX, &self.0)
    }

    #[must_use]
    pub fn decode(value: &str) -> Option<Self> {
        decode_b64(REVISION_ID_PREFIX, value).map(Self)
    }
}

impl std::fmt::Display for SourceSnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.encode())
    }
}

impl std::fmt::Display for RevisionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.encode())
    }
}

/// The identity a finalized [`Program`] carries (never serialized — units
/// cannot know it, and packs recompute a fallback at load; see
/// `Program::identity`'s `#[borsh(skip)]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramIdentity {
    pub revision_id: RevisionId,
    pub source_snapshot_id: SourceSnapshotId,
    /// Canonical toolchain string: `<version>+<channel>[+<commit>]`, or
    /// `dev+<blake3(exe)>` for untagged dev builds.
    pub compiler_id: String,
    /// Count of pool functions stamped by [`assign_function_ids`].
    pub function_count: u32,
}

/// One user source file's identity, recorded by the emit finalizer so the
/// revision dictionary's file table (§4.2 `FileRow`) can be built from a
/// pure `Program` walk. Rides `Program::source_files` (`#[borsh(skip)]`,
/// same rationale as `identity`); empty for fallback-identity programs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFileIdentity {
    /// Project-relative path (the same form `Function::source_file` uses).
    pub path: String,
    /// BLAKE3-256 of the file contents.
    pub content_hash: [u8; 32],
}

/// §4.3: `source_snapshot_id` over per-file BLAKE3 hashes.
///
/// `files` must be the project's user source files (builtin stubs excluded —
/// they are toolchain, covered by `compiler_id`), sorted by project-relative
/// path, each with the BLAKE3-256 of its contents. `manifest_hash` is the
/// BLAKE3-256 of `baml.toml` when the project has one.
#[must_use]
pub fn source_snapshot_id(
    files: &[(String, [u8; 32])],
    manifest_hash: Option<[u8; 32]>,
) -> SourceSnapshotId {
    debug_assert!(
        files.windows(2).all(|w| w[0].0 <= w[1].0),
        "snapshot files must be sorted by project-relative path"
    );
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"baml.snapshot.v1\0");
    hasher.update(&(files.len() as u64).to_le_bytes());
    for (path, content_hash) in files {
        hasher.update(&(u32::try_from(path.len()).unwrap_or(u32::MAX)).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update(content_hash);
    }
    match manifest_hash {
        Some(hash) => {
            hasher.update(&[1u8]);
            hasher.update(&hash);
        }
        None => {
            hasher.update(&[0u8]);
        }
    }
    SourceSnapshotId(*hasher.finalize().as_bytes())
}

/// §4.3: `revision_id` = snapshot × toolchain × options.
#[must_use]
pub fn revision_id(
    source_snapshot_id: SourceSnapshotId,
    compiler_id: &str,
    opt_level: u8,
    emit_test_cases: bool,
) -> RevisionId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"baml.revision.v1\0");
    hasher.update(&source_snapshot_id.0);
    hasher.update(&(u32::try_from(compiler_id.len()).unwrap_or(u32::MAX)).to_le_bytes());
    hasher.update(compiler_id.as_bytes());
    hasher.update(&[opt_level, u8::from(emit_test_cases)]);
    hasher.update(&0u16.to_le_bytes());
    RevisionId(*hasher.finalize().as_bytes())
}

/// §4.3 fallback for identity-less programs (legacy packs, hand-built test
/// programs): domain-separated BLAKE3 of `borsh(Program)`, computed once at
/// engine init. Distinct domain so a fallback id can never collide with a
/// source-derived one.
#[must_use]
pub fn fallback_revision_id(program_borsh: &[u8]) -> (RevisionId, SourceSnapshotId) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"baml.revision.fallback.v1\0");
    hasher.update(program_borsh);
    let hash = *hasher.finalize().as_bytes();
    // The fallback has no source view, so snapshot == revision under the
    // fallback domain: same bytes, both explicitly non-source-derived.
    (RevisionId(hash), SourceSnapshotId(hash))
}

/// Cross-revision structural identity of one definition (design §4.5).
/// Carried on `Function` (borsh — a deliberate wire bump) so identity
/// survives into packs and the dictionary is a pure `Program` walk.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DefinitionMeta {
    /// THE cross-revision join key: `"function:user.Extract"`,
    /// `"lambda:function:user.hello.retry#0"`, ... Emitted from HIR/MIR
    /// ItemRefs, never parsed from display names.
    pub definition_key: String,
    /// Owner type for methods: `"class:user.Foo"`.
    pub owner_type_key: Option<String>,
    /// Structural lambda identity; `None` for named functions.
    pub lambda: Option<LambdaIdentity>,
}

/// Structural identity of a lambda/closure (design §4.5): parent + lowering
/// ordinal + kind, carried instead of re-parsing `"<lambda(parent, N)>"`
/// debug strings.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LambdaIdentity {
    /// The enclosing definition's key: `"function:user.hello.retry"`.
    pub parent_definition_key: String,
    /// Lowering order within the parent body (stable for unchanged source:
    /// bodies relower whole; clean units are reused verbatim).
    pub ordinal: u32,
    pub kind: LambdaKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum LambdaKind {
    /// An ordinary source lambda.
    Lambda,
    /// A spawn body / tagged-body closure.
    SpawnedClosure,
    /// A compiler-synthesized adapter closure.
    Adapter,
}

impl LambdaIdentity {
    /// The derived definition key: `lambda:{parent_key}#{ordinal}`.
    #[must_use]
    pub fn definition_key(&self) -> String {
        format!("lambda:{}#{}", self.parent_definition_key, self.ordinal)
    }
}

/// §4.4 `def_content_hash`: BLAKE3-256 of one function's *behavior* —
/// signature + a canonicalized bytecode projection.
///
/// The projection excludes everything display-only (line tables, spans,
/// local names, debug metadata, docstrings, display strings) — **and
/// canonicalizes inter-object references**: `GlobalIndex` / `ObjectIndex`
/// operands are whole-program layout, so hashing them raw would churn every
/// unchanged function's hash on any unrelated add/remove. Each distinct
/// index operand is rewritten to its first-occurrence ordinal, and the
/// referents' canonical names (definition keys, global slot names) are
/// hashed alongside in ordinal order. The pinned golden test is "edit an
/// unrelated file ⇒ all other def_content_hashes byte-identical".
///
/// Uses `relink::visit_index_operands` — the exhaustive operand walk whose
/// match fails compilation when a new operand-carrying opcode appears, so
/// no reference can silently escape canonicalization.
pub struct DefHashResolver<'p> {
    program: &'p Program,
    /// Reverse of `function_global_indices` + `let_global_indices`:
    /// global slot → dotted FQN.
    global_names: std::collections::HashMap<usize, &'p str>,
}

impl<'p> DefHashResolver<'p> {
    #[must_use]
    pub fn new(program: &'p Program) -> DefHashResolver<'p> {
        let mut global_names = std::collections::HashMap::new();
        for (name, &slot) in &program.function_global_indices {
            global_names.insert(slot, name.as_str());
        }
        for (name, &slot) in &program.let_global_indices {
            global_names.insert(slot, name.as_str());
        }
        DefHashResolver {
            program,
            global_names,
        }
    }

    /// Canonical, layout-independent name for one referenced object.
    fn object_name(&self, index: crate::ObjectIndex) -> String {
        match self.program.objects.get(index.raw()) {
            Some(Object::Function(f)) => f
                .def_meta
                .as_ref()
                .map(|meta| meta.definition_key.clone())
                .unwrap_or_else(|| format!("function:{}", f.name)),
            Some(Object::Class(c)) => format!("class:{}", c.name.display_name()),
            Some(Object::Enum(e)) => format!("enum:{}", e.name.display_name()),
            Some(Object::Interface(i)) => format!("interface:{}", i.name.display_name()),
            Some(Object::String(s)) => format!("str:{s}"),
            // Exotic pool kinds (packages, impl rules, runtime-only shapes).
            // A stable kind tag keeps the hash deterministic; these are not
            // cross-file references, so layout churn does not reach them in
            // practice.
            Some(other) => {
                let kind = match other {
                    Object::Package(_) => "Package",
                    Object::Function(_)
                    | Object::Class(_)
                    | Object::Enum(_)
                    | Object::Interface(_)
                    | Object::String(_) => unreachable!("handled above"),
                    Object::ImplRule(_) => "ImplRule",
                    Object::Instance(_) => "Instance",
                    Object::Variant(_) => "Variant",
                    Object::Closure(_) => "Closure",
                    Object::BoundMethod(_) => "BoundMethod",
                    Object::GenericFunction(_) => "GenericFunction",
                    Object::HostClosure(_) => "HostClosure",
                    Object::Cell(_) => "Cell",
                    Object::Bigint(_) => "Bigint",
                    Object::Array(_) => "Array",
                    Object::Map(_) => "Map",
                    Object::Float(_) => "Float",
                    Object::Future(_) => "Future",
                    Object::UnscheduledFuture(_) => "UnscheduledFuture",
                    Object::RustData(_) => "RustData",
                    Object::Collector(_) => "Collector",
                    Object::Type(_) => "Type",
                    Object::Uint8Array(_) => "Uint8Array",
                };
                format!("obj:{kind}")
            }
            None => format!("obj:missing#{}", index.raw()),
        }
    }

    /// Compute the §4.4 hash for one function of this program.
    #[must_use]
    pub fn def_content_hash(&self, function: &Function) -> [u8; 32] {
        use borsh::BorshSerialize as _;

        // Clone, then rewrite every cross-function index operand to its
        // first-occurrence ordinal while recording the canonical referent
        // names in ordinal order. Cold path (dictionary build), one clone
        // per function per revision.
        let mut projected = function.clone();
        let mut names: Vec<String> = Vec::new();
        let mut global_ordinals: std::collections::HashMap<usize, u32> =
            std::collections::HashMap::new();
        let mut object_ordinals: std::collections::HashMap<usize, u32> =
            std::collections::HashMap::new();
        crate::relink::visit_index_operands(&mut projected, |operand| match operand {
            crate::relink::IndexOperand::Global(slot) => {
                let raw = slot.raw();
                let next = names.len() as u32;
                let ordinal = *global_ordinals.entry(raw).or_insert_with(|| {
                    names.push(format!(
                        "global:{}",
                        self.global_names.get(&raw).copied().unwrap_or("?")
                    ));
                    next
                });
                *slot = crate::GlobalIndex::from_raw(ordinal as usize);
            }
            crate::relink::IndexOperand::Object(index) => {
                let raw = index.raw();
                let next = names.len() as u32;
                let ordinal = *object_ordinals.entry(raw).or_insert_with(|| {
                    names.push(self.object_name(crate::ObjectIndex::from_raw(raw)));
                    next
                });
                *index = crate::ObjectIndex::from_raw(ordinal as usize);
            }
        });

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"baml.def.v1\0");
        // Signature: kind (borsh via the FunctionKindWire proxy — carries
        // the SysOp identity, collapses native pointers), arity, types.
        let mut buf = Vec::new();
        projected.kind.serialize(&mut buf).expect("kind serializes");
        hasher.update(&buf);
        hasher.update(&(u32::try_from(function.arity).unwrap_or(u32::MAX)).to_le_bytes());
        let mut buf = Vec::new();
        projected
            .param_types
            .serialize(&mut buf)
            .expect("types serialize");
        projected
            .return_type
            .serialize(&mut buf)
            .expect("type serializes");
        projected
            .throws_type
            .serialize(&mut buf)
            .expect("type serializes");
        hasher.update(&buf);

        // The canonicalized behavior projection: instructions + function-
        // local tables, with rewritten ordinals; then the referent names.
        let mut buf = Vec::new();
        projected
            .bytecode
            .instructions
            .serialize(&mut buf)
            .expect("bytecode serializes");
        projected
            .bytecode
            .constants
            .serialize(&mut buf)
            .expect("constants serialize");
        projected
            .bytecode
            .jump_tables
            .serialize(&mut buf)
            .expect("jump tables serialize");
        projected
            .bytecode
            .field_copy_sets
            .serialize(&mut buf)
            .expect("field copies serialize");
        projected
            .bytecode
            .class_init_plans
            .serialize(&mut buf)
            .expect("init plans serialize");
        projected
            .bytecode
            .match_hash_tables
            .serialize(&mut buf)
            .expect("match tables serialize");
        projected
            .bytecode
            .exception_table
            .serialize(&mut buf)
            .expect("exception table serializes");
        projected
            .bytecode
            .handler_context_table
            .serialize(&mut buf)
            .expect("handler table serializes");
        hasher.update(&buf);

        let mut buf = Vec::new();
        names.serialize(&mut buf).expect("names serialize");
        hasher.update(&buf);

        *hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Function;

    fn test_function(name: &str) -> Function {
        Function {
            name: name.to_string(),
            source_file: String::new(),
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode: crate::bytecode::Bytecode::default(),
            kind: crate::types::FunctionKind::Bytecode,
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
            origin: crate::types::FunctionOrigin::UserDefined,
            body_meta: None,
            capture: Default::default(),
            def_meta: None,
            function_id: 0,
        }
    }

    #[test]
    fn assign_is_dense_pool_ordered_and_idempotent() {
        let mut program = Program::default();
        program
            .objects
            .push(Object::Function(Box::new(test_function("a"))));
        program
            .objects
            .push(Object::String("not a function".into()));
        program
            .objects
            .push(Object::Function(Box::new(test_function("b"))));

        let count = assign_function_ids(&mut program);
        assert_eq!(count, 2);
        let ids: Vec<u32> = program
            .objects
            .iter()
            .filter_map(|obj| match obj {
                Object::Function(f) => Some(f.function_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec![FIRST_POOL_FUNCTION_ID, FIRST_POOL_FUNCTION_ID + 1]
        );
        assert!(verify_function_ids(&program));

        // Idempotent.
        let count2 = assign_function_ids(&mut program);
        assert_eq!(count2, 2);
        assert!(verify_function_ids(&program));
    }

    #[test]
    fn reserved_range_is_below_pool_ids() {
        assert!(FUNCTION_ID_UNKNOWN < FIRST_POOL_FUNCTION_ID);
        assert!(FUNCTION_ID_SPAWN_CLOSURE < FIRST_POOL_FUNCTION_ID);
        assert_eq!(FUNCTION_ID_UNKNOWN, 0, "existing sentinel is load-bearing");
    }

    #[test]
    fn id_string_forms_roundtrip() {
        let rev = RevisionId([7; 32]);
        let src = SourceSnapshotId([9; 32]);
        assert!(rev.encode().starts_with("baml_rev_1_"));
        assert!(src.encode().starts_with("baml_src_1_"));
        assert_eq!(RevisionId::decode(&rev.encode()), Some(rev));
        assert_eq!(SourceSnapshotId::decode(&src.encode()), Some(src));
        assert_eq!(
            RevisionId::decode(&src.encode()),
            None,
            "prefixes are typed"
        );
    }

    #[test]
    fn snapshot_hash_is_order_and_content_sensitive() {
        let a = ("a.baml".to_string(), [1u8; 32]);
        let b = ("b.baml".to_string(), [2u8; 32]);
        let base = source_snapshot_id(&[a.clone(), b.clone()], None);
        // Content change changes the id.
        let changed = source_snapshot_id(&[a.clone(), ("b.baml".to_string(), [3u8; 32])], None);
        assert_ne!(base, changed);
        // Manifest presence changes the id.
        let with_manifest = source_snapshot_id(&[a, b], Some([4u8; 32]));
        assert_ne!(base, with_manifest);
    }

    #[test]
    fn revision_id_covers_toolchain_and_options() {
        let snapshot = source_snapshot_id(&[("a.baml".to_string(), [1u8; 32])], None);
        let base = revision_id(snapshot, "0.15.0+canary", 2, false);
        assert_ne!(base, revision_id(snapshot, "0.15.1+canary", 2, false));
        assert_ne!(base, revision_id(snapshot, "0.15.0+canary", 0, false));
        assert_ne!(base, revision_id(snapshot, "0.15.0+canary", 2, true));
        // Fallback ids live in their own domain.
        let (fallback, _) = fallback_revision_id(b"anything");
        assert_ne!(base, fallback);
    }

    #[test]
    fn def_content_hash_is_layout_independent() {
        use crate::bytecode::Instruction;

        // A function that references a global slot and a pool object. Its
        // hash must survive both indices shifting (the "edit an unrelated
        // file" golden property, §4.4).
        fn build(program_shift: usize) -> ([u8; 32], Program) {
            let mut program = Program::default();
            // Shift the pool with unrelated leading objects.
            for i in 0..program_shift {
                program
                    .objects
                    .push(Object::String(format!("pad-{i}").into()));
            }
            let mut callee = test_function("user.callee");
            callee.def_meta = Some(DefinitionMeta {
                definition_key: "function:user.callee".to_string(),
                owner_type_key: None,
                lambda: None,
            });
            let callee_idx = program.objects.len();
            program.objects.push(Object::Function(Box::new(callee)));

            let mut caller = test_function("user.caller");
            caller.bytecode.instructions = vec![
                Instruction::LoadGlobal(crate::GlobalIndex::from_raw(7 + program_shift)),
                Instruction::Return,
            ];
            caller
                .bytecode
                .constants
                .push(crate::types::ConstValue::Object(
                    crate::ObjectIndex::from_raw(callee_idx),
                ));
            program
                .function_global_indices
                .insert("user.callee".to_string(), 7 + program_shift);
            let caller_for_hash = caller.clone();
            program.objects.push(Object::Function(Box::new(caller)));

            let resolver = DefHashResolver::new(&program);
            (resolver.def_content_hash(&caller_for_hash), program)
        }

        let (hash_a, _) = build(0);
        let (hash_b, _) = build(5);
        assert_eq!(
            hash_a, hash_b,
            "shifting unrelated pool/global layout must not change the hash"
        );

        // But changing the *referent's identity* must change it.
        let (hash_c, _) = {
            let mut program = Program::default();
            let mut callee = test_function("user.other_callee");
            callee.def_meta = Some(DefinitionMeta {
                definition_key: "function:user.other_callee".to_string(),
                owner_type_key: None,
                lambda: None,
            });
            let callee_idx = program.objects.len();
            program.objects.push(Object::Function(Box::new(callee)));
            let mut caller = test_function("user.caller");
            caller.bytecode.instructions = vec![
                Instruction::LoadGlobal(crate::GlobalIndex::from_raw(7)),
                Instruction::Return,
            ];
            caller
                .bytecode
                .constants
                .push(crate::types::ConstValue::Object(
                    crate::ObjectIndex::from_raw(callee_idx),
                ));
            program
                .function_global_indices
                .insert("user.other_callee".to_string(), 7);
            let caller_for_hash = caller.clone();
            program.objects.push(Object::Function(Box::new(caller)));
            let resolver = DefHashResolver::new(&program);
            (resolver.def_content_hash(&caller_for_hash), program)
        };
        assert_ne!(hash_a, hash_c, "different referent identity must differ");
    }

    /// §4.4 documented non-transitivity (Q1 gate): changing a CALLEE'S
    /// BODY does not change the caller's local hash — referenced
    /// definitions contribute their names, never their contents. Equal
    /// local hashes are therefore not proof of equal effective behavior.
    #[test]
    fn def_content_hash_is_not_transitive_over_callee_bodies() {
        use crate::bytecode::Instruction;

        fn build(callee_body: Vec<Instruction>) -> [u8; 32] {
            let mut program = Program::default();
            let mut callee = test_function("user.callee");
            callee.bytecode.instructions = callee_body;
            callee.def_meta = Some(DefinitionMeta {
                definition_key: "function:user.callee".to_string(),
                owner_type_key: None,
                lambda: None,
            });
            let callee_idx = program.objects.len();
            program.objects.push(Object::Function(Box::new(callee)));
            let mut caller = test_function("user.caller");
            caller.bytecode.instructions = vec![
                Instruction::LoadGlobal(crate::GlobalIndex::from_raw(7)),
                Instruction::Return,
            ];
            caller
                .bytecode
                .constants
                .push(crate::types::ConstValue::Object(
                    crate::ObjectIndex::from_raw(callee_idx),
                ));
            program
                .function_global_indices
                .insert("user.callee".to_string(), 7);
            let caller_for_hash = caller.clone();
            program.objects.push(Object::Function(Box::new(caller)));
            let resolver = DefHashResolver::new(&program);
            resolver.def_content_hash(&caller_for_hash)
        }

        let with_short_callee = build(vec![Instruction::Return]);
        let with_long_callee = build(vec![
            Instruction::LoadGlobal(crate::GlobalIndex::from_raw(3)),
            Instruction::Return,
        ]);
        assert_eq!(
            with_short_callee, with_long_callee,
            "a callee body change must NOT change the caller's local hash"
        );
    }

    #[test]
    fn lambda_definition_key_shape() {
        let lambda = LambdaIdentity {
            parent_definition_key: "function:user.hello.retry".to_string(),
            ordinal: 2,
            kind: LambdaKind::Lambda,
        };
        assert_eq!(
            lambda.definition_key(),
            "lambda:function:user.hello.retry#2"
        );
    }
}
