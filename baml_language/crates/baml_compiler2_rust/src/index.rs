//! Linked program metadata for Rust emission.

use std::collections::{HashMap, VecDeque, hash_map::Entry};

use crate::coords::{
    DeclCoords, class_coords, class_head_coords, declared_function_coords, enum_coords,
    enum_head_coords, function_coords, interface_head_coords, let_coords,
};
use baml_base::SourceRoot;
use baml_compiler2_emit::function_identity;
use baml_compiler2_hir::{
    file_package::file_package,
    loc::{DeclRef, FunctionLoc, LetLoc},
    package::edge_table,
};
use baml_compiler2_hir_ty::extern_loc::{ClassRef, EnumRef, FunctionRef, GlobalRef};
use baml_compiler2_mir::{FunctionOwner, MirFunctionId};
use baml_type::{DeclName, typetag::TypeTag};
use bex_vm_types::{
    Bytecode, ConstValue, DeclPath, FnPath, FunctionKind, Object, ObjectIndex, Program, TypeHead,
    bytecode::BlockPcs,
    types::{FunctionIdentity, LocalName, PackageKind},
};

pub(crate) struct FunctionInfo {
    pub(crate) kind: FunctionKind,
    /// The slots a call lays its arguments over (`Function::argument_layout`).
    pub(crate) layout: baml_type::CallLayout,
    pub(crate) native_key: Option<String>,
    pub(crate) arity: usize,
    /// Whether calls inherit the caller's `$trace` controls.
    pub(crate) trace_passthrough: bool,
    /// Whether the returned turn is charged as model usage when the call
    /// completes (`FunctionMeta::ModelInvoke`).
    pub(crate) model_invoke: bool,
}

/// Package and declaration path uniquely identify an object; rendered names do not.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeclKey {
    package: u32,
    path: DeclPath,
}

/// Lambdas have no package-table entry, so lookup uses their full identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LambdaKey {
    package: u32,
    identity: FunctionIdentity,
}

#[derive(Default)]
struct ObjectInfo {
    function: Option<FunctionInfo>,
    name: String,
    display: String,
    head: Option<TypeTag>,
    class_tag: Option<i64>,
    enum_variants: Option<usize>,
    type_constants: Vec<(usize, ConstValue)>,
    positions: Option<Positions>,
}

#[derive(Default)]
pub(crate) struct ProgramIndex {
    /// Package ordinals derived from the linked dependency edges.
    packages: HashMap<SourceRoot, u32>,
    /// Function object by identity.
    functions: HashMap<DeclKey, usize>,
    /// Class (object, field count) by identity.
    classes: HashMap<DeclKey, (usize, usize)>,
    /// Enum object by identity.
    enums: HashMap<DeclKey, usize>,
    /// Type heads anchored by declaration identity.
    interfaces: HashMap<DeclKey, usize>,
    aliases: HashMap<DeclKey, usize>,
    /// The global slot of each top-level `let`, by identity.
    lets: HashMap<DeclKey, usize>,
    /// The zero-argument initializer helper of each top-level binding.
    let_helpers: HashMap<DeclKey, usize>,
    /// Lambda (closure body) object by identity; `None` for an identity
    /// more than one object claims, which then stays bytecode.
    lambdas: HashMap<LambdaKey, Option<usize>>,
    objects: Vec<ObjectInfo>,
    /// Method names conservatively resolve all possible dispatch targets.
    methods: HashMap<String, Vec<usize>>,
    /// The methods and interface bodies of every non-stdlib package.
    pub(crate) user_methods: Vec<usize>,
    /// String literal → the compile-time string object bytecode loads it from.
    pub(crate) strings: HashMap<String, usize>,
    /// Global slot used by pooled generic function values.
    pub(crate) globals: HashMap<usize, usize>,
    /// Find a pooled generic function with the program's anchored type arguments.
    generic_functions: Vec<(usize, Box<[bex_vm_types::RealizedTy]>, usize)>,
    /// Bigint literal (as decimal text) → its pooled `Object::Bigint`.
    pub(crate) bigints: HashMap<String, usize>,
    /// Byte array literal → its pooled `Object::Uint8Array` template, which
    /// bytecode deep-copies on every evaluation.
    pub(crate) byte_arrays: HashMap<Vec<u8>, usize>,
}

/// Source and shield metadata for MIR positions in a bytecode function.
pub(crate) struct Positions(Bytecode);

impl Positions {
    fn of(bytecode: &Bytecode) -> Self {
        Self(Bytecode {
            line_table: bytecode.line_table.clone(),
            shield_table: bytecode.shield_table.clone(),
            block_pcs: bytecode.block_pcs.clone(),
            ..Bytecode::default()
        })
    }

    /// The PCs of MIR block `block`.
    pub(crate) fn block(&self, block: usize) -> Option<&BlockPcs> {
        self.0.block_pcs.get(block)
    }

    /// Whether the bytecode carries no MIR positions: they are compile-time
    /// only, so a deserialized program has none.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.block_pcs.is_empty()
    }

    /// Both backends must lower the same blocks and statements.
    pub(crate) fn fit(&self, body: &baml_compiler2_mir::MirFunctionBody<'_>) -> bool {
        self.0.block_pcs.len() == body.blocks.len()
            && self
                .0
                .block_pcs
                .iter()
                .zip(&body.blocks)
                .all(|(pcs, block)| pcs.statements.len() == block.statements.len())
    }

    /// Source line and shield state at `pc`.
    pub(crate) fn reading(&self, pc: usize) -> (usize, bool) {
        (self.0.source_line_for_pc(pc), self.0.pc_is_shielded(pc))
    }
}

impl ProgramIndex {
    pub(crate) fn new(
        db: &dyn baml_compiler2_mir::Db,
        root: SourceRoot,
        program: &Program,
    ) -> Self {
        let mut index = Self {
            packages: package_ordinals(db, root, program),
            objects: (0..program.objects.len())
                .map(|_| ObjectInfo::default())
                .collect(),
            ..Self::default()
        };
        index.index_objects(program);
        index.index_globals(program);
        index.index_bytecode(program);
        index.index_declarations(program);
        index
    }

    fn index_objects(&mut self, program: &Program) {
        for (object, item) in program.objects.iter().enumerate() {
            match item {
                Object::Function(function) => {
                    let Some(name) = program.function_name(ObjectIndex::from_raw(object)) else {
                        continue;
                    };
                    // Hand-built programs may omit package ranges.
                    let placing = u32::try_from(object).ok().and_then(|raw| {
                        (0u32..)
                            .zip(&program.packages)
                            .find(|(_, package)| package.objects.contains(&raw))
                    });
                    let display = match &placing {
                        Some((_, package)) if name.starts_with('.') => {
                            format!("{}{name}", package.name)
                        }
                        _ => name.clone(),
                    };
                    if let (FunctionIdentity::Synthetic { .. }, Some((package, _))) =
                        (&function.identity, &placing)
                    {
                        let key = LambdaKey {
                            package: *package,
                            identity: function.identity.clone(),
                        };
                        match self.lambdas.entry(key) {
                            Entry::Vacant(vacant) => {
                                vacant.insert(Some(object));
                            }
                            Entry::Occupied(mut occupied) => {
                                debug_assert!(false, "two lambdas claim {:?}", occupied.key());
                                occupied.insert(None);
                            }
                        }
                    }
                    if let (FunctionIdentity::InitLet(path), Some((package, _))) =
                        (&function.identity, &placing)
                    {
                        self.let_helpers.insert(
                            DeclKey {
                                package: *package,
                                path: DeclPath::Let(path.clone()),
                            },
                            object,
                        );
                    }
                    self.objects[object].name = name;
                    let method = match &function.identity {
                        FunctionIdentity::Declared(FnPath::Method { name, .. }) => Some(name),
                        FunctionIdentity::InterfaceBody(_) => function.identity.declared_name(),
                        _ => None,
                    };
                    if let Some(method) = method {
                        self.methods
                            .entry(method.as_str().to_owned())
                            .or_default()
                            .push(object);
                        let user = program
                            .package_placing(ObjectIndex::from_raw(object))
                            .is_some_and(|package| package.kind == PackageKind::Ordinary);
                        if user {
                            self.user_methods.push(object);
                        }
                    }
                    self.objects[object].display = display;
                    self.objects[object].function = Some(FunctionInfo {
                        kind: function.kind,
                        layout: function.argument_layout(),
                        native_key: function.native_key.as_deref().map(str::to_owned),
                        arity: function.arity,
                        trace_passthrough: function.trace_passthrough,
                        model_invoke: matches!(
                            function.body_meta,
                            Some(bex_vm_types::FunctionMeta::ModelInvoke)
                        ),
                    });
                }
                Object::String(text) => {
                    self.strings.entry(text.to_string()).or_insert(object);
                }
                Object::GenericFunction(generic) => {
                    self.generic_functions.push((
                        generic.function.raw(),
                        generic.type_args.clone(),
                        object,
                    ));
                }
                Object::Bigint(value) => {
                    self.bigints.entry(value.to_string()).or_insert(object);
                }
                Object::Uint8Array(bytes) => {
                    self.byte_arrays
                        .entry(bytes.lock().clone())
                        .or_insert(object);
                }
                Object::Class(class) => {
                    self.objects[object].head = Some(class.type_tag);
                    self.objects[object].class_tag = Some(class.type_tag.as_i64());
                }
                Object::Enum(declaration) => {
                    self.objects[object].head = Some(declaration.type_tag);
                    self.objects[object].enum_variants = Some(declaration.variants.len());
                }
                _ => {}
            }
        }
    }

    fn index_globals(&mut self, program: &Program) {
        for (slot, global) in program.globals.iter().enumerate() {
            if let ConstValue::Object(object) = global
                && self
                    .objects
                    .get(object.raw())
                    .is_some_and(|info| info.function.is_some())
            {
                self.globals.entry(object.raw()).or_insert(slot);
            }
        }
    }

    fn index_bytecode(&mut self, program: &Program) {
        for (object, item) in program.objects.iter().enumerate() {
            let Object::Function(function) = item else {
                continue;
            };
            if !matches!(function.kind, FunctionKind::Bytecode) {
                continue;
            }
            let constants: Vec<(usize, ConstValue)> = function
                .bytecode
                .constants
                .iter()
                .enumerate()
                .filter(|(_, constant)| {
                    matches!(constant, ConstValue::Type(_) | ConstValue::Literal(_))
                })
                .map(|(i, constant)| (i, constant.clone()))
                .collect();
            if !constants.is_empty() {
                self.objects[object].type_constants = constants;
            }
            self.objects[object].positions = Some(Positions::of(&function.bytecode));
        }
    }

    /// Each table is keyed by its package's own declaration maps, so no two
    /// objects can claim one identity.
    fn index_declarations(&mut self, program: &Program) {
        for (ordinal, package) in (0u32..).zip(&program.packages) {
            let key = |path: DeclPath| DeclKey {
                package: ordinal,
                path,
            };
            // A function's cell holds its object; a `let`'s holds its value.
            for (path, &cell) in &package.globals {
                let slot = package.slot_base.raw() + cell as usize;
                if matches!(path, DeclPath::Let(_)) {
                    self.lets.insert(key(path.clone()), slot);
                    continue;
                }
                if !matches!(path, DeclPath::Function(_) | DeclPath::InterfaceBody(_)) {
                    continue;
                }
                let Some(ConstValue::Object(object)) = program.globals.get(slot) else {
                    continue;
                };
                if self
                    .objects
                    .get(object.raw())
                    .is_some_and(|info| info.function.is_some())
                {
                    self.functions.insert(key(path.clone()), object.raw());
                }
            }
            for (path, &object) in &package.classes {
                if let Object::Class(class) = &program.objects[object] {
                    self.classes.insert(
                        key(DeclPath::Class(path.clone())),
                        (object.raw(), class.fields.len()),
                    );
                }
            }
            for (path, &object) in &package.enums {
                self.enums
                    .insert(key(DeclPath::Enum(path.clone())), object.raw());
            }
            for (path, &object) in &package.interfaces {
                self.interfaces
                    .insert(key(DeclPath::Interface(path.clone())), object.raw());
            }
            for (path, &object) in &package.type_aliases {
                self.aliases
                    .insert(key(DeclPath::TypeAlias(path.clone())), object.raw());
            }
        }
    }

    pub(crate) fn lambda<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        identity: &MirFunctionId<'db>,
    ) -> Option<usize> {
        let package = *self.packages.get(&identity_root(db, identity))?;
        let key = LambdaKey {
            package,
            identity: function_identity(db, identity),
        };
        self.lambdas.get(&key).copied().flatten()
    }

    pub(crate) fn declared(
        &self,
        db: &dyn baml_compiler2_mir::Db,
        function: FunctionLoc<'_>,
    ) -> Option<usize> {
        self.lookup(&self.functions, declared_function_coords(db, function)?)
    }

    pub(crate) fn function<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        function: FunctionRef<'db>,
    ) -> Option<(usize, &FunctionInfo)> {
        let object = self.lookup(&self.functions, function_coords(db, function)?)?;
        Some((object, self.info(object)))
    }

    /// The (object, field count) of `class`, if the program has it.
    pub(crate) fn class<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
    ) -> Option<(usize, usize)> {
        self.lookup(&self.classes, class_coords(db, class))
    }

    /// [`Self::class`] for the class a type template's head names.
    pub(crate) fn class_at(&self, head: &baml_type::DeclName) -> Option<(usize, usize)> {
        self.lookup(&self.classes, class_head_coords(head))
    }

    /// The object of the interface a dispatch template's head names.
    pub(crate) fn interface_at(&self, head: &baml_type::DeclName) -> Option<usize> {
        self.lookup(&self.interfaces, interface_head_coords(head))
    }

    /// [`Self::enum_`] for the enum a type template's head names.
    pub(crate) fn enum_at(&self, head: &baml_type::DeclName) -> Option<usize> {
        self.lookup(&self.enums, enum_head_coords(head))
    }

    /// Find a pooled generic function with the program's anchored type arguments.
    pub(crate) fn pooled_generic_function(
        &self,
        object: usize,
        type_args: &[baml_compiler2_mir::RealizedTy],
    ) -> Option<usize> {
        let global = *self.globals.get(&object)?;
        let head_of = |decl: &baml_type::DeclName| {
            let object = self
                .class_at(decl)
                .map(|(object, _)| object)
                .or_else(|| self.enum_at(decl))?;
            self.objects.get(object)?.head
        };
        let mut known = true;
        for ty in type_args {
            ty.visit_heads(&mut |decl| known &= head_of(decl).is_some());
        }
        if !known {
            return None;
        }
        let anchored: Vec<bex_vm_types::RealizedTy> = type_args
            .iter()
            .map(|ty| {
                ty.map_heads(&mut |decl| {
                    bex_vm_types::TypeHead::unresolved(head_of(decl).expect("checked above"))
                })
            })
            .collect();
        self.generic_functions
            .iter()
            .find(|(slot, args, _)| *slot == global && args[..] == anchored[..])
            .map(|&(_, _, object)| object)
    }

    /// The object of `enum_ref`, if the program has it.
    pub(crate) fn enum_<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        enum_ref: EnumRef<'db>,
    ) -> Option<usize> {
        self.lookup(&self.enums, enum_coords(db, enum_ref))
    }

    /// The global slot of the top-level `let` `binding`, if the program has
    /// it.
    pub(crate) fn let_slot<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        binding: GlobalRef<'db>,
    ) -> Option<usize> {
        self.lookup(&self.lets, let_coords(db, binding))
    }

    /// The linked initializer helper of the top-level binding.
    pub(crate) fn let_helper<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        binding: LetLoc<'db>,
    ) -> Option<usize> {
        self.lookup(&self.let_helpers, let_coords(db, DeclRef::Source(binding)))
    }

    /// Ordinal of the package that owns the function.
    pub(crate) fn package_of(&self, root: SourceRoot) -> Option<u32> {
        self.packages.get(&root).copied()
    }

    pub(crate) fn class_tag(&self, object: usize) -> i64 {
        self.objects[object].class_tag.expect("class metadata")
    }

    pub(crate) fn enum_variants(&self, object: usize) -> usize {
        self.objects[object].enum_variants.expect("enum metadata")
    }

    /// Replace declaration heads with their linked tags for constant-pool lookup.
    /// Return `None` if any declaration is absent.
    pub(crate) fn anchor(
        &self,
        template: &baml_compiler2_mir::TyTemplate,
    ) -> Option<bex_vm_types::TyTemplate> {
        let mut known = true;
        let anchored = template.map_heads(&mut |head: &DeclName| {
            let object = self.head_object(head);
            known &= object.is_some();
            TypeHead::unresolved(TypeTag::of_static_index(object.unwrap_or(0)))
        });
        known.then_some(anchored)
    }

    /// The object of the declaration `head` names: a class, enum, interface
    /// or recursive alias.
    fn head_object(&self, head: &DeclName) -> Option<usize> {
        let path = item_path(head);
        let root = head.root();
        self.lookup(&self.classes, (root, DeclPath::Class(path.clone())))
            .map(|(object, _)| object)
            .or_else(|| self.lookup(&self.enums, (root, DeclPath::Enum(path.clone()))))
            .or_else(|| self.lookup(&self.interfaces, (root, DeclPath::Interface(path.clone()))))
            .or_else(|| self.lookup(&self.aliases, (root, DeclPath::TypeAlias(path))))
    }

    /// Find a matching constant in the bytecode function's pool.
    pub(crate) fn pooled_constant(&self, function: usize, wanted: &ConstValue) -> Option<usize> {
        self.objects
            .get(function)?
            .type_constants
            .iter()
            .find(|(_, constant)| constant == wanted)
            .map(|(index, _)| *index)
    }

    /// Where the MIR positions of the bytecode function at `object` landed.
    pub(crate) fn positions(&self, object: usize) -> &Positions {
        self.objects[object]
            .positions
            .as_ref()
            .expect("bytecode positions")
    }

    pub(crate) fn info(&self, object: usize) -> &FunctionInfo {
        self.objects[object]
            .function
            .as_ref()
            .expect("function metadata")
    }

    /// The name of the function at `object` as the loader renders it.
    pub(crate) fn name(&self, object: usize) -> &str {
        &self.objects[object].name
    }

    /// Every method and interface body named `method`, whichever class or
    /// interface declares it: what a dispatch of that name can reach.
    pub(crate) fn methods_named(&self, method: &str) -> impl Iterator<Item = usize> + '_ {
        self.methods.get(method).into_iter().flatten().copied()
    }

    /// [`Self::name`] under its package's label; see
    /// [`crate::CompiledFunction::display`].
    pub(crate) fn display(&self, object: usize) -> &str {
        &self.objects[object].display
    }

    fn lookup<T: Copy>(&self, table: &HashMap<DeclKey, T>, (root, path): DeclCoords) -> Option<T> {
        let package = *self.packages.get(&root)?;
        table.get(&DeclKey { package, path }).copied()
    }
}

fn item_path(head: &DeclName) -> LocalName {
    LocalName {
        namespace: head.namespace().clone(),
        name: head.name().clone(),
    }
}

/// Follow synthetic owners to their enclosing declaration's source root.
fn identity_root(db: &dyn baml_compiler2_mir::Db, identity: &MirFunctionId<'_>) -> SourceRoot {
    match identity {
        MirFunctionId::Declared(function) => file_package(db, function.file(db)).root,
        MirFunctionId::Synthetic { parent, .. } => match parent.as_ref() {
            FunctionOwner::Function(function) => file_package(db, function.file(db)).root,
            FunctionOwner::Let(binding) => file_package(db, binding.file(db)).root,
            FunctionOwner::Synthetic(owner) => identity_root(db, owner),
        },
    }
}

/// Match linked dependency edges to source roots. Unplaced packages stay unresolved.
fn package_ordinals(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
) -> HashMap<SourceRoot, u32> {
    let mut ordinals = HashMap::from([(root, program.root)]);
    let mut queue = VecDeque::from([(program.root, root)]);
    while let Some((ordinal, source)) = queue.pop_front() {
        let Some(package) = program.packages.get(ordinal as usize) else {
            continue;
        };
        let edges = edge_table(db, source);
        for edge in &package.edges {
            let Some((_, target)) = edges.iter().find(|(name, _)| *name == edge.name) else {
                continue;
            };
            if let Entry::Vacant(vacant) = ordinals.entry(*target) {
                vacant.insert(edge.target);
                queue.push_back((edge.target, *target));
            }
        }
    }
    ordinals
}
