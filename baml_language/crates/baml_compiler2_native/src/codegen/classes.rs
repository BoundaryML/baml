//! The objects that say what a program's types are, placed in the
//! executable's data: a class object for each class and enum, an object
//! for each variant of an enum, a type object for each type a call passes
//! as a type argument, and the package objects class objects name. Their
//! layouts are in [`layout`].
//!
//! An instance holds the address of its class's object and an enum's
//! variant that of its enum's, and the object holds the row dispatch
//! reads: one code address per column of the program. A value that is
//! neither has a tag, and a table per column holds what each tag has
//! there. A column is a dispatched method's, or an interface's that a
//! type test asks about, which holds one where the type implements it.
//!
//! A class of `baml.panics` is a panic: its instances carry
//! [`layout::HEADER_PANIC`] in their headers, which is what a handler that
//! takes every error tests to let one pass.

use std::collections::HashMap;

use baml_compiler2_hir::package::lang_roots;
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    layout::{
        class_fields, class_generic_params, class_head, class_ref_of, enum_head, enum_ref_of,
        enum_variants, interface_ref_of, lang_class, package_classes,
    },
    render::Viewpoint,
};
use baml_compiler2_mir::{RuntimeTy, TyTemplateInterface};
use cranelift_module::{DataDescription, DataId, FuncId, Module};

use super::{Backend, Declared, Translator, internal, table_word};
use crate::{
    CompileError, layout,
    narrow::TestKind,
    program::{Code, Entry, FnKey, Program},
    types::{NativeTy, native_ty},
};

/// What a class object is of.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ClassLike<'db> {
    Class(ClassRef<'db>),
    Enum(EnumRef<'db>),
}

/// A class object, which is defined once every column is known.
#[derive(Clone, Copy)]
pub(super) struct ClassObject<'db> {
    what: ClassLike<'db>,
    pub(super) data: DataId,
    /// Where a class's description is in the section of descriptions.
    description: u32,
    /// An enum's variants: their objects, one after another in declaration
    /// order, and the table of their addresses.
    variants: Option<(DataId, DataId)>,
}

/// What a type object is of: the type itself where its native form says
/// all of it, and otherwise the type as the program spells it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum TypeKey<'db> {
    Native(NativeTy<'db>),
    Spelled(String),
}

/// The classes the runtime knows by position.
pub(super) struct WellKnown {
    /// The class object of each class of [`layout::WELL_KNOWN`], in its
    /// order.
    classes: Vec<usize>,
    /// The class object of each class of [`layout::NAMED`], in its order:
    /// none until the program calls a native whose declaration names it.
    named: [Option<usize>; layout::NAMED.len()],
}

/// What a handler's list or a switch on a tag names: a tag, or a class
/// object by its number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Taken {
    Tag(usize),
    Class(usize),
}

/// The values that pass a test.
#[derive(Default)]
pub(super) struct Passing {
    /// Those with one of these tags. A string that is a slice passes
    /// where [`layout::TAG_STRING`] is listed.
    pub(super) tags: Vec<usize>,
    /// The instances and the variants of these class objects.
    pub(super) classes: Vec<usize>,
    /// Those whose row holds something in one of these columns.
    pub(super) interfaces: Vec<usize>,
    /// Every panic passes: decided by the flag of the instance's header.
    pub(super) panics: bool,
    pub(super) null: bool,
}

/// Whether `class` is a panic: a class of `baml.panics`.
pub(crate) fn is_panic_class<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class: ClassRef<'db>,
) -> bool {
    class_head(db, class).is_panic_type(lang_roots(db))
}

/// The tag every value of type `ty` has, when the tag alone says a value
/// is of the type. A string that is a slice has the tag after the one
/// given here.
fn fixed_tag(ty: &NativeTy<'_>) -> Option<usize> {
    Some(match ty {
        NativeTy::Int => layout::TAG_INT,
        NativeTy::Float => layout::TAG_FLOAT,
        NativeTy::Bool => layout::TAG_BOOL,
        NativeTy::String => layout::TAG_STRING,
        NativeTy::Bigint => layout::TAG_BIGINT,
        NativeTy::Bytes => layout::TAG_BYTES,
        _ => return None,
    })
}

/// The class or the enum values of type `ty` have the class object of.
fn class_like<'db>(ty: &NativeTy<'db>) -> Option<ClassLike<'db>> {
    match ty {
        NativeTy::Class(class, _) => Some(ClassLike::Class(*class)),
        NativeTy::Enum(enum_ref) => Some(ClassLike::Enum(*enum_ref)),
        _ => None,
    }
}

/// Write `value` at `offset` of `bytes`.
fn put<const N: usize>(bytes: &mut [u8], offset: usize, value: [u8; N]) {
    bytes[offset..offset + N].copy_from_slice(&value);
}

impl<'db> Backend<'db> {
    /// The header of a new instance of `class`.
    pub(super) fn class_header(db: &'db dyn baml_compiler2_mir::Db, class: ClassRef<'db>) -> usize {
        if is_panic_class(db, class) {
            layout::TAG_INSTANCE | layout::HEADER_PANIC
        } else {
            layout::TAG_INSTANCE
        }
    }

    /// The class object of a class or an enum, by its number, declared the
    /// first time it is asked for. A class is described then, and an
    /// enum's variants are placed.
    pub(super) fn class_object(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        what: ClassLike<'db>,
    ) -> Result<usize, CompileError> {
        if let Some(index) = self.class_ids.get(&what) {
            return Ok(*index);
        }
        if self.class_objects_defined {
            return Err(internal("every class object is met before any is defined"));
        }
        let data = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        let index = self.class_objects.len();
        self.class_objects.push(ClassObject {
            what,
            data,
            description: 0,
            variants: None,
        });
        self.class_ids.insert(what, index);
        match what {
            ClassLike::Class(class) => {
                let at = self.json.class_description(db, class);
                self.class_objects[index].description = at;
                self.identify_described(db)?;
            }
            ClassLike::Enum(enum_ref) => {
                let variants = self.define_variants(db, enum_ref, data)?;
                self.class_objects[index].variants = Some(variants);
            }
        }
        Ok(index)
    }

    /// The data of the class object numbered `index`.
    pub(super) fn class_data(&self, index: usize) -> DataId {
        self.class_objects[index].data
    }

    /// The objects of the variants of the enum whose class object is
    /// numbered `index`: one after another, [`layout::VARIANT_SIZE`] bytes
    /// each.
    pub(super) fn variants_data(&self, index: usize) -> Result<DataId, CompileError> {
        self.class_objects[index]
            .variants
            .map(|(objects, _)| objects)
            .ok_or_else(|| internal("an enum's class object has its variants"))
    }

    /// Place the objects of an enum's variants and the table of their
    /// addresses.
    fn define_variants(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        enum_ref: EnumRef<'db>,
        class: DataId,
    ) -> Result<(DataId, DataId), CompileError> {
        let variants = enum_variants(db, enum_ref);
        let objects = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        // Never empty: a symbol needs a byte to be the address of.
        let mut bytes = vec![0; (variants.len() * layout::VARIANT_SIZE).max(layout::VALUE)];
        let mut description = DataDescription::new();
        let class = self.module.declare_data_in_data(class, &mut description);
        for (index, variant) in variants.iter().enumerate() {
            let at = index * layout::VARIANT_SIZE;
            put(&mut bytes, at, layout::TAG_ENUM_VARIANT.to_ne_bytes());
            description.write_data_addr(table_word(at + layout::VARIANT_ENUM), class, 0);
            put(&mut bytes, at + layout::VARIANT_INDEX, index.to_ne_bytes());
            let name = self.string_constant(variant.as_str())?;
            let name = self.module.declare_data_in_data(name, &mut description);
            description.write_data_addr(table_word(at + layout::VARIANT_NAME), name, 0);
        }
        self.define_data(objects, description, bytes)?;

        let table = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        let mut description = DataDescription::new();
        let placed = self.module.declare_data_in_data(objects, &mut description);
        for index in 0..variants.len() {
            description.write_data_addr(
                table_word(index * layout::WORD),
                placed,
                i64::try_from(index * layout::VARIANT_SIZE).map_err(internal)?,
            );
        }
        let bytes = vec![0; (variants.len() * layout::WORD).max(layout::WORD)];
        self.define_data(table, description, bytes)?;
        Ok((objects, table))
    }

    /// The object of the package called `name`.
    fn package_object(&mut self, name: &str) -> Result<DataId, CompileError> {
        if let Some(id) = self.packages.get(name) {
            return Ok(*id);
        }
        let mut bytes = vec![0; layout::PACKAGE_SIZE];
        put(&mut bytes, 0, layout::TAG_PACKAGE.to_ne_bytes());
        let mut description = DataDescription::new();
        let text = self.string_constant(name)?;
        let text = self.module.declare_data_in_data(text, &mut description);
        description.write_data_addr(table_word(layout::PACKAGE_NAME), text, 0);
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        self.define_data(id, description, bytes)?;
        self.packages.insert(name.to_owned(), id);
        Ok(id)
    }

    /// The column that says whether a type implements `interface`, given
    /// one the first time a test asks: after the dispatched methods'.
    fn interface_column(&mut self, methods: usize, interface: &TyTemplateInterface) -> usize {
        let known = self
            .interface_columns
            .iter()
            .position(|known| known == interface);
        methods
            + known.unwrap_or_else(|| {
                self.interface_columns.push(interface.clone());
                self.interface_columns.len() - 1
            })
    }

    /// The table that holds what each tag has in `column`: a word per tag,
    /// the tag's type id being the number of its word.
    pub(super) fn tag_row(&mut self, column: usize) -> Result<DataId, CompileError> {
        if let Some(id) = self.tag_rows.get(&column) {
            return Ok(*id);
        }
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        self.tag_rows.insert(column, id);
        Ok(id)
    }

    /// Define every class object and every table of tags: each row holds
    /// the code its type has for each dispatched method, entered as a call
    /// that names it enters it, and one in the column of each interface
    /// it implements.
    pub(super) fn define_rows(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        program: &Program<'db>,
        declared: &HashMap<FnKey<'db>, Declared<'db>>,
    ) -> Result<(), CompileError> {
        // What each class object and each tag has, by column: code, or
        // nothing for the one of an interface.
        let mut of_classes: HashMap<usize, Vec<(usize, Option<FuncId>)>> = HashMap::new();
        let mut of_tags: Vec<(usize, usize, Option<FuncId>)> = Vec::new();
        for (ty, methods) in &program.rows {
            for (slot, code) in methods {
                let function = match code {
                    Code::Function(key) => {
                        let callee = declared
                            .get(key)
                            .ok_or_else(|| internal("an implementation is part of the program"))?;
                        if callee.entry != Entry::Direct || callee.type_args != 0 {
                            return Err(internal(
                                "an implementation is entered with its values alone",
                            ));
                        }
                        callee.id
                    }
                    Code::Native(native) => self.dispatched_stub(db, native)?,
                };
                let column = program.slots[*slot].column;
                match (fixed_tag(ty), class_like(ty)) {
                    (Some(tag), _) => of_tags.push((tag, column, Some(function))),
                    // No value of a class nothing made an object for.
                    (None, Some(what)) => {
                        if let Some(index) = self.class_ids.get(&what) {
                            of_classes
                                .entry(*index)
                                .or_default()
                                .push((column, Some(function)));
                        }
                    }
                    (None, None) => {
                        return Err(internal(
                            "a row is for a type a tag or a class object names",
                        ));
                    }
                }
            }
        }
        // The program is whole: the implementors of an interface are among
        // the types it makes values of.
        let interfaces = self.interface_columns.clone();
        let mut implemented: HashMap<(usize, usize), bool> = HashMap::new();
        for (position, interface) in interfaces.iter().enumerate() {
            let column = program.columns + position;
            for ty in &program.types {
                let implements = crate::dispatch::implements(db, ty, interface);
                if let Some(tag) = fixed_tag(ty) {
                    if implements {
                        of_tags.push((tag, column, None));
                    }
                } else if let Some(index) =
                    class_like(ty).and_then(|what| self.class_ids.get(&what).copied())
                {
                    // One row serves every instantiation of a class.
                    if implemented
                        .insert((index, column), implements)
                        .is_some_and(|known| known != implements)
                    {
                        return Err(internal(
                            "a type test of an interface that some instantiations of one class \
                             implement and others do not",
                        ));
                    }
                }
            }
        }
        for ((index, column), implements) in implemented {
            if implements {
                of_classes.entry(index).or_default().push((column, None));
            }
        }

        let columns = program.columns + interfaces.len();
        self.class_objects_defined = true;
        for index in 0..self.class_objects.len() {
            let row = of_classes.remove(&index).unwrap_or_default();
            self.define_class_object(db, index, columns, &row)?;
        }
        let tables: Vec<(usize, DataId)> = self.tag_rows.iter().map(|(c, id)| (*c, *id)).collect();
        for (column, id) in tables {
            let mut bytes = vec![0; layout::TAGS_END * layout::WORD];
            let mut description = DataDescription::new();
            for (tag, _, code) in of_tags.iter().filter(|(_, at, _)| *at == column) {
                // A slice is a string: it has what a string has.
                let tags = [*tag, layout::TAG_STRING_SLICE];
                let tags = &tags[..=usize::from(*tag == layout::TAG_STRING)];
                for tag in tags {
                    match code {
                        Some(function) => {
                            let function = self
                                .module
                                .declare_func_in_data(*function, &mut description);
                            description
                                .write_function_addr(table_word(*tag * layout::WORD), function);
                        }
                        None => put(&mut bytes, *tag * layout::WORD, 1_usize.to_ne_bytes()),
                    }
                }
            }
            self.define_data(id, description, bytes)?;
        }
        Ok(())
    }

    /// Define the class object numbered `index`, whose row has `columns`
    /// columns and holds `row`.
    fn define_class_object(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: usize,
        columns: usize,
        row: &[(usize, Option<FuncId>)],
    ) -> Result<(), CompileError> {
        let ClassObject {
            what,
            data,
            description: described,
            variants,
        } = self.class_objects[index];
        let (head, slots, generics, kind, described) = match what {
            ClassLike::Class(class) => {
                let generics = class_generic_params(db, class).len();
                let panic = if is_panic_class(db, class) {
                    layout::CLASS_PANIC
                } else {
                    0
                };
                (
                    class_head(db, class),
                    generics + class_fields(db, class).len(),
                    generics,
                    layout::CLASS_KIND_CLASS | panic,
                    described,
                )
            }
            ClassLike::Enum(enum_ref) => (
                enum_head(db, enum_ref),
                enum_variants(db, enum_ref).len(),
                0,
                layout::CLASS_KIND_ENUM,
                self.json.listed_enum(enum_ref).unwrap_or(0),
            ),
        };
        let mut bytes = vec![0; layout::class_size(columns)];
        let header = layout::TAG_CLASS | columns << layout::HEADER_PAYLOAD_SHIFT;
        put(&mut bytes, 0, header.to_ne_bytes());
        put(
            &mut bytes,
            layout::CLASS_SLOT_COUNT,
            table_word(slots).to_ne_bytes(),
        );
        put(
            &mut bytes,
            layout::CLASS_GENERIC_COUNT,
            table_word(generics).to_ne_bytes(),
        );
        put(&mut bytes, layout::CLASS_KIND, kind.to_ne_bytes());
        put(
            &mut bytes,
            layout::CLASS_INDEX,
            table_word(index).to_ne_bytes(),
        );

        let mut description = DataDescription::new();
        let package = Viewpoint::canonical(db)
            .display_label(head.root())
            .to_string();
        let package = self.package_object(&package)?;
        let package = self.module.declare_data_in_data(package, &mut description);
        description.write_data_addr(table_word(layout::CLASS_PACKAGE), package, 0);
        let mut name = String::new();
        for part in head.namespace() {
            name.push_str(part.as_str());
            name.push('.');
        }
        name.push_str(head.name().as_str());
        let name = self.string_constant(&name)?;
        let name = self.module.declare_data_in_data(name, &mut description);
        description.write_data_addr(table_word(layout::CLASS_NAME), name, 0);
        if described != 0 {
            let types = self
                .module
                .declare_data_in_data(self.types, &mut description);
            description.write_data_addr(
                table_word(layout::CLASS_DESCRIPTION),
                types,
                i64::try_from(layout::PROGRAM_DESCRIPTIONS + described as usize)
                    .map_err(internal)?,
            );
        }
        if let Some((_, table)) = variants {
            let table = self.module.declare_data_in_data(table, &mut description);
            description.write_data_addr(table_word(layout::CLASS_VARIANTS), table, 0);
        }
        for (column, code) in row {
            let at = layout::CLASS_METHODS + column * layout::WORD;
            match code {
                Some(function) => {
                    let function = self
                        .module
                        .declare_func_in_data(*function, &mut description);
                    description.write_function_addr(table_word(at), function);
                }
                None => put(&mut bytes, at, 1_usize.to_ne_bytes()),
            }
        }
        self.define_data(data, description, bytes)
    }

    /// The tag and the class object a type object of `ty` has: what says
    /// a value is of the type, when a tag and a class object do.
    fn type_prefix(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &NativeTy<'db>,
    ) -> Result<(usize, Option<usize>), CompileError> {
        Ok(match ty {
            NativeTy::Enum(enum_ref) => (
                layout::TAG_ENUM_VARIANT,
                Some(self.class_object(db, ClassLike::Enum(*enum_ref))?),
            ),
            // The class object of a class with type parameters is every
            // instantiation's.
            NativeTy::Class(class, arguments) if arguments.is_empty() => (
                layout::TAG_INSTANCE,
                Some(self.class_object(db, ClassLike::Class(*class))?),
            ),
            other => (fixed_tag(other).unwrap_or(0), None),
        })
    }

    /// The type object of the type `ty`, and where its description is in
    /// the section of descriptions: zero for a type `baml.json` has no
    /// form for.
    pub(super) fn type_object_of(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &NativeTy<'db>,
    ) -> Result<(DataId, u32), CompileError> {
        let key = TypeKey::Native(ty.clone());
        if let Some(known) = self.type_objects.get(&key) {
            return Ok(*known);
        }
        let (tag, class) = self.type_prefix(db, ty)?;
        let mut described = self.json.describe_native_if_any(db, ty);
        self.identify_described(db)?;
        if described == 0
            && let Some(class) = class
        {
            // What its class object has: how a thrown instance is written.
            described = self.class_objects[class].description;
        }
        self.define_type_object(key, tag, class, described)
    }

    /// The type object of the type `ty` as the program spells it, and
    /// where its description is.
    pub(super) fn type_object_spelled(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &RuntimeTy,
    ) -> Result<(DataId, u32), CompileError> {
        let native = native_ty(db, ty, &[]);
        // A type its native form says all of has one object, however a
        // call comes to name it.
        let plain = match ty {
            RuntimeTy::Int
            | RuntimeTy::Float
            | RuntimeTy::Bool
            | RuntimeTy::String
            | RuntimeTy::Bigint
            | RuntimeTy::Uint8Array => true,
            RuntimeTy::Enum(head) => enum_ref_of(db, head).is_some(),
            RuntimeTy::Class(head, arguments) => {
                arguments.is_empty()
                    && interface_ref_of(db, head).is_none()
                    && class_ref_of(db, head).is_some()
            }
            _ => false,
        };
        if plain && let Some(native) = &native {
            return self.type_object_of(db, native);
        }
        let key = TypeKey::Spelled(format!("{ty:?}"));
        if let Some(known) = self.type_objects.get(&key) {
            return Ok(*known);
        }
        let described = self.json.describe_if_any(db, ty);
        self.identify_described(db)?;
        // A described type is decided by its description, which says more
        // than a tag: a literal's value, a union's members.
        let (tag, class) = match &native {
            Some(native) if described == 0 => self.type_prefix(db, native)?,
            _ => (0, None),
        };
        self.define_type_object(key, tag, class, described)
    }

    /// The type object no value is of the type of: what a call passes for
    /// a type argument that names no type native code holds values of.
    pub(super) fn type_object_of_nothing(&mut self) -> Result<DataId, CompileError> {
        let key = TypeKey::Spelled(String::new());
        if let Some((known, _)) = self.type_objects.get(&key) {
            return Ok(*known);
        }
        Ok(self.define_type_object(key, 0, None, 0)?.0)
    }

    fn define_type_object(
        &mut self,
        key: TypeKey<'db>,
        tag: usize,
        class: Option<usize>,
        described: u32,
    ) -> Result<(DataId, u32), CompileError> {
        let mut bytes = vec![0; layout::TYPE_SIZE];
        put(&mut bytes, 0, layout::TAG_TYPE.to_ne_bytes());
        put(&mut bytes, layout::TYPE_TAG, tag.to_ne_bytes());
        let mut description = DataDescription::new();
        if let Some(class) = class {
            let class = self.class_objects[class].data;
            let class = self.module.declare_data_in_data(class, &mut description);
            description.write_data_addr(table_word(layout::TYPE_CLASS), class, 0);
        }
        if described != 0 {
            let types = self
                .module
                .declare_data_in_data(self.types, &mut description);
            description.write_data_addr(
                table_word(layout::TYPE_DESCRIPTION),
                types,
                i64::try_from(layout::PROGRAM_DESCRIPTIONS + described as usize)
                    .map_err(internal)?,
            );
        }
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        self.define_data(id, description, bytes)?;
        self.type_objects.insert(key, (id, described));
        Ok((id, described))
    }

    /// The class objects of the classes the runtime knows by position,
    /// made the first time.
    pub(super) fn well_known(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<&WellKnown, CompileError> {
        if self.well_known.is_none() {
            let mut classes = Vec::with_capacity(layout::WELL_KNOWN.len());
            for (namespace, name) in layout::WELL_KNOWN {
                let class = lang_class(db, baml_base::LangPackage::Baml, &[namespace], name)
                    .ok_or_else(|| {
                        internal(format!(
                            "the standard library declares `baml.{namespace}.{name}`"
                        ))
                    })?;
                classes.push(self.class_object(db, ClassLike::Class(class))?);
            }
            // The header's flag stands for the whole of `baml.panics`, so
            // the runtime's list must be all of it.
            let root = lang_roots(db)
                .get(baml_base::LangPackage::Baml)
                .ok_or_else(|| internal("the standard library is installed"))?;
            let panics = package_classes(db, root)
                .filter(|class| is_panic_class(db, *class))
                .count();
            if panics != layout::WELL_KNOWN_PANICS {
                return Err(internal(
                    "the runtime's list of panics is the standard library's",
                ));
            }
            self.well_known = Some(WellKnown {
                classes,
                named: [None; layout::NAMED.len()],
            });
        }
        self.well_known
            .as_ref()
            .ok_or_else(|| internal("the well-known classes were just found"))
    }

    /// Give every class the declaration of `native` names a class object
    /// and a description, for the native to be passed an instance or to
    /// build one, and note each the runtime finds by position. A class one
    /// of them holds in a field is described with it, and the native finds
    /// it there: one that builds a `Timeout` builds the `Duration` its
    /// field says.
    pub(super) fn name_classes(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        native: &crate::natives::Native,
    ) -> Result<(), CompileError> {
        for path in native.classes {
            let class = crate::natives::class_at(db, *path)
                .ok_or_else(|| internal(format!("the standard library declares `{}`", path.1)))?;
            let index = self.class_object(db, ClassLike::Class(class))?;
            let listed = layout::NAMED.iter().position(|(namespace, name)| {
                lang_class(db, baml_base::LangPackage::Baml, &[namespace], name) == Some(class)
            });
            if let Some(position) = listed {
                self.well_known(db)?;
                if let Some(known) = &mut self.well_known {
                    known.named[position] = Some(index);
                }
            }
        }
        Ok(())
    }

    /// Emit the program's table of types: the address of the class object
    /// of each class the runtime knows by position, and then the section
    /// of descriptions, in which the linker writes the address of each
    /// entry and of each class object a description names.
    pub(super) fn define_types(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<(), CompileError> {
        let known = self.well_known(db)?;
        let listed: Vec<Option<usize>> = known
            .classes
            .iter()
            .copied()
            .map(Some)
            .chain(known.named)
            .collect();
        let json = std::mem::replace(&mut self.json, crate::json::JsonTypes::new());
        let mut description = DataDescription::new();
        for (position, class) in listed.into_iter().enumerate() {
            if let Some(class) = class {
                let class = self.class_objects[class].data;
                let class = self.module.declare_data_in_data(class, &mut description);
                description.write_data_addr(
                    table_word(layout::PROGRAM_CLASSES + position * layout::WORD),
                    class,
                    0,
                );
            }
        }
        let start = table_word(layout::PROGRAM_DESCRIPTIONS);
        for (at, entry) in json.addresses() {
            let entry = self
                .module
                .declare_func_in_data(FuncId::from_u32(*entry), &mut description);
            description.write_function_addr(start + at, entry);
        }
        for (at, class) in json.class_words() {
            let class = self.class_objects[*class].data;
            let class = self.module.declare_data_in_data(class, &mut description);
            description.write_data_addr(start + at, class, 0);
        }
        let mut bytes = vec![0; layout::PROGRAM_DESCRIPTIONS];
        bytes.extend(json.section());
        self.define_data(self.types, description, bytes)
    }
}

impl<'db> Translator<'_, 'db> {
    /// The values that pass `test`, with every panic class taken out of
    /// the list when all of them pass: the header's flag says that. An
    /// interface is its column where `by_column`, and otherwise each type
    /// of the program that implements it.
    pub(super) fn passing(
        &mut self,
        test: &TestKind<'db>,
        by_column: bool,
    ) -> Result<Passing, CompileError> {
        let mut passing = Passing::default();
        self.add_passing(test, by_column, &mut passing)?;
        let panics = self.panic_classes()?;
        passing.panics |= panics.iter().all(|class| passing.classes.contains(class));
        if passing.panics {
            passing.classes.retain(|class| !panics.contains(class));
        }
        passing.tags.sort_unstable();
        passing.tags.dedup();
        passing.classes.sort_unstable();
        passing.classes.dedup();
        passing.interfaces.sort_unstable();
        passing.interfaces.dedup();
        Ok(passing)
    }

    fn add_passing(
        &mut self,
        test: &TestKind<'db>,
        by_column: bool,
        passing: &mut Passing,
    ) -> Result<(), CompileError> {
        match test {
            TestKind::Exactly(NativeTy::Null) => passing.null = true,
            TestKind::AnyArray => passing.tags.push(layout::TAG_ARRAY),
            TestKind::Exactly(ty) => {
                if !self.add_passing_type(ty, passing)? {
                    return Err(internal(
                        "a checked type test is of a type a tag or a class object names",
                    ));
                }
            }
            // Compared with the type object the function was passed.
            TestKind::Argument(_) => {}
            TestKind::Panic => passing.panics = true,
            TestKind::Is(_) => {
                return Err(internal("a checked literal test compares values"));
            }
            TestKind::AnyOf(tests) => {
                for test in tests {
                    self.add_passing(test, by_column, passing)?;
                }
            }
            TestKind::Implements(interface) if by_column => {
                let column = self
                    .backend
                    .interface_column(self.program.columns, interface);
                passing.interfaces.push(column);
            }
            // The program is whole: the implementors are the types it makes
            // values of.
            TestKind::Implements(interface) => {
                let program = self.program;
                for ty in &program.types {
                    if crate::dispatch::implements(self.db, ty, interface) {
                        self.add_passing_type(ty, passing)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Add the values of type `ty` to `passing`, when a tag or a class
    /// object says a value is one.
    fn add_passing_type(
        &mut self,
        ty: &NativeTy<'db>,
        passing: &mut Passing,
    ) -> Result<bool, CompileError> {
        if let Some(tag) = fixed_tag(ty) {
            passing.tags.push(tag);
        } else if let Some(what) = class_like(ty) {
            let class = self.backend.class_object(self.db, what)?;
            passing.classes.push(class);
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// The class object of `class`, which takes no type arguments.
    pub(super) fn plain_class(&mut self, class: ClassRef<'db>) -> Result<usize, CompileError> {
        if !class_generic_params(self.db, class).is_empty() {
            return Err(internal(
                "a checked class tag is of a class without type parameters",
            ));
        }
        self.backend.class_object(self.db, ClassLike::Class(class))
    }

    /// The class object of every panic class.
    pub(super) fn panic_classes(&mut self) -> Result<Vec<usize>, CompileError> {
        let known = self.backend.well_known(self.db)?;
        Ok(known.classes[..layout::WELL_KNOWN_PANICS].to_vec())
    }
}
