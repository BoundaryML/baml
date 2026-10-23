//! Linked program metadata for Rust emission.

use std::collections::{HashMap, VecDeque, hash_map::Entry};

use crate::coords::{
    DeclCoords, class_coords, class_head_coords, declared_function_coords, enum_coords,
    enum_head_coords, function_coords, interface_head_coords, item_path, let_coords,
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
    Bytecode, ConstValue, DeclPath, FnPath, Object, ObjectIndex, Program, TypeHead,
    types::{FunctionIdentity, PackageKind},
};

pub(crate) struct ProgramIndex<'p> {
    program: &'p Program,
    names: Vec<(String, String)>,
    /// Package ordinals derived from the linked dependency edges.
    packages: HashMap<SourceRoot, u32>,
    let_helpers: HashMap<(u32, DeclPath), usize>,
    /// Lambda (closure body) object by identity; `None` for an identity
    /// more than one object claims, which then stays bytecode.
    lambdas: HashMap<(u32, FunctionIdentity), Option<usize>>,
    /// Method names conservatively resolve all possible dispatch targets.
    methods: HashMap<String, Vec<usize>>,
    /// The methods and interface bodies of every non-stdlib package.
    pub(crate) user_methods: Vec<usize>,
    /// String literal → the compile-time string object bytecode loads it from.
    pub(crate) strings: HashMap<String, usize>,
    /// Global slot used by pooled generic function values.
    pub(crate) globals: HashMap<usize, usize>,
    /// Find a pooled generic function with the program's anchored type arguments.
    generic_functions: Vec<(usize, &'p [bex_vm_types::RealizedTy], usize)>,
    pub(crate) bigints: HashMap<String, usize>,
    /// Byte array literal → its pooled `Object::Uint8Array` template, which
    /// bytecode deep-copies on every evaluation.
    pub(crate) byte_arrays: HashMap<Vec<u8>, usize>,
}

impl<'p> ProgramIndex<'p> {
    pub(crate) fn new(
        db: &dyn baml_compiler2_mir::Db,
        root: SourceRoot,
        program: &'p Program,
    ) -> Self {
        let mut index = Self {
            program,
            names: vec![(String::new(), String::new()); program.objects.len()],
            packages: package_ordinals(db, root, program),
            let_helpers: HashMap::new(),
            lambdas: HashMap::new(),
            methods: HashMap::new(),
            user_methods: Vec::new(),
            strings: HashMap::new(),
            globals: HashMap::new(),
            generic_functions: Vec::new(),
            bigints: HashMap::new(),
            byte_arrays: HashMap::new(),
        };
        index.index_objects(program);
        index.index_globals(program);
        index
    }

    fn index_objects(&mut self, program: &'p Program) {
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
                        let key = (*package, function.identity.clone());
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
                        self.let_helpers
                            .insert((*package, DeclPath::Let(path.clone())), object);
                    }
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
                        if placing.is_some_and(|(_, package)| package.kind == PackageKind::Ordinary)
                        {
                            self.user_methods.push(object);
                        }
                    }
                    self.names[object] = (name, display);
                }
                Object::String(text) => {
                    self.strings.entry(text.to_string()).or_insert(object);
                }
                Object::GenericFunction(generic) => {
                    self.generic_functions.push((
                        generic.function.raw(),
                        &generic.type_args,
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
                _ => {}
            }
        }
    }

    fn index_globals(&mut self, program: &Program) {
        for (slot, global) in program.globals.iter().enumerate() {
            if let ConstValue::Object(object) = global
                && matches!(
                    self.program.objects.get(object.raw()),
                    Some(Object::Function(_))
                )
            {
                self.globals.entry(object.raw()).or_insert(slot);
            }
        }
    }

    pub(crate) fn lambda<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        identity: &MirFunctionId<'db>,
    ) -> Option<usize> {
        let package = *self.packages.get(&identity_root(db, identity))?;
        let key = (package, function_identity(db, identity));
        self.lambdas.get(&key).copied().flatten()
    }

    pub(crate) fn declared(
        &self,
        db: &dyn baml_compiler2_mir::Db,
        function: FunctionLoc<'_>,
    ) -> Option<usize> {
        self.lookup(declared_function_coords(db, function)?)
    }

    pub(crate) fn function<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        function: FunctionRef<'db>,
    ) -> Option<(usize, &bex_vm_types::types::Function)> {
        let object = self.lookup(function_coords(db, function)?)?;
        Some((object, self.info(object)))
    }

    pub(crate) fn class<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
    ) -> Option<(usize, usize)> {
        self.class_fields(class_coords(db, class))
    }

    pub(crate) fn class_at(&self, head: &baml_type::DeclName) -> Option<(usize, usize)> {
        self.class_fields(class_head_coords(head))
    }

    pub(crate) fn interface_at(&self, head: &baml_type::DeclName) -> Option<usize> {
        self.lookup(interface_head_coords(head))
    }

    pub(crate) fn enum_at(&self, head: &baml_type::DeclName) -> Option<usize> {
        self.lookup(enum_head_coords(head))
    }

    fn class_fields(&self, coords: DeclCoords) -> Option<(usize, usize)> {
        let object = self.lookup(coords)?;
        let Object::Class(class) = &self.program.objects[ObjectIndex::from_raw(object)] else {
            return None;
        };
        Some((object, class.fields.len()))
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
            self.head_tag(object)
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

    pub(crate) fn enum_<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        enum_ref: EnumRef<'db>,
    ) -> Option<usize> {
        self.lookup(enum_coords(db, enum_ref))
    }

    pub(crate) fn let_slot<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        binding: GlobalRef<'db>,
    ) -> Option<usize> {
        self.lookup(let_coords(db, binding))
    }

    pub(crate) fn let_helper<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        binding: LetLoc<'db>,
    ) -> Option<usize> {
        self.let_helpers
            .get(&self.key(let_coords(db, DeclRef::Source(binding)))?)
            .copied()
    }

    pub(crate) fn package_of(&self, root: SourceRoot) -> Option<u32> {
        self.packages.get(&root).copied()
    }

    fn head_tag(&self, object: usize) -> Option<TypeTag> {
        match &self.program.objects[ObjectIndex::from_raw(object)] {
            Object::Class(class) => Some(class.type_tag),
            Object::Enum(decl) => Some(decl.type_tag),
            _ => None,
        }
    }

    pub(crate) fn class_tag(&self, object: usize) -> i64 {
        self.head_tag(object).expect("class metadata").as_i64()
    }

    pub(crate) fn enum_variants(&self, object: usize) -> usize {
        let Object::Enum(decl) = &self.program.objects[ObjectIndex::from_raw(object)] else {
            unreachable!("enum metadata")
        };
        decl.variants.len()
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
        [
            DeclPath::Class,
            DeclPath::Enum,
            DeclPath::Interface,
            DeclPath::TypeAlias,
        ]
        .into_iter()
        .find_map(|path| self.lookup((head.root(), path(item_path(head)))))
    }

    pub(crate) fn pooled_constant(&self, function: usize, wanted: &ConstValue) -> Option<usize> {
        self.info(function)
            .bytecode
            .constants
            .iter()
            .position(|constant| constant == wanted)
    }

    pub(crate) fn positions(&self, object: usize) -> &Bytecode {
        &self.info(object).bytecode
    }

    pub(crate) fn info(&self, object: usize) -> &bex_vm_types::types::Function {
        let Object::Function(function) = &self.program.objects[ObjectIndex::from_raw(object)]
        else {
            unreachable!("function metadata")
        };
        function
    }

    pub(crate) fn name(&self, object: usize) -> &str {
        &self.names[object].0
    }

    /// Every method and interface body named `method`, whichever class or
    /// interface declares it: what a dispatch of that name can reach.
    pub(crate) fn methods_named(&self, method: &str) -> impl Iterator<Item = usize> + '_ {
        self.methods.get(method).into_iter().flatten().copied()
    }

    /// [`Self::name`] under its package's label; see
    /// [`crate::CompiledFunction::display`].
    pub(crate) fn display(&self, object: usize) -> &str {
        &self.names[object].1
    }

    fn key(&self, (root, path): DeclCoords) -> Option<(u32, DeclPath)> {
        Some((*self.packages.get(&root)?, path))
    }

    fn lookup(&self, coords: DeclCoords) -> Option<usize> {
        let (ordinal, path) = self.key(coords)?;
        let package = self.program.packages.get(ordinal as usize)?;
        let object = match &path {
            DeclPath::Class(name) => *package.classes.get(name)?,
            DeclPath::Enum(name) => *package.enums.get(name)?,
            DeclPath::Interface(name) => *package.interfaces.get(name)?,
            DeclPath::TypeAlias(name) => *package.type_aliases.get(name)?,
            DeclPath::Let(_) => {
                return Some(package.slot_base.raw() + *package.globals.get(&path)? as usize);
            }
            DeclPath::Function(_) | DeclPath::InterfaceBody(_) => {
                let slot = package.slot_base.raw() + *package.globals.get(&path)? as usize;
                let ConstValue::Object(object) = self.program.globals.get(slot)? else {
                    return None;
                };
                if !matches!(
                    self.program.objects.get(object.raw()),
                    Some(Object::Function(_))
                ) {
                    return None;
                }
                *object
            }
        };
        Some(object.raw())
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
