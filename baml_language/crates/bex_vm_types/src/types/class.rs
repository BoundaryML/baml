use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{
    HeapPtr, ObjectIndex, RuntimeTy,
    types::{InstanceLayout, TypeValue},
};

/// A field within a runtime class, carrying type and schema metadata.
// Each bool is one flag attribute of the declaration.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one independent flag per attribute the source can write; no combination is invalid"
)]
pub struct ClassField {
    pub name: String,
    /// Used by paths that don't care about parametric class type-args
    /// (codegen, `sys_ops` walking, output-format rendering).  For typed
    /// runtime walking against an instance's type arguments, use
    /// `field_template` and call `substitute` on it instead.
    pub field_type: RuntimeTy,
    /// Field-type template with `TypeArgRef(N)` leaves for class-level
    /// generic params (`N` indexes into the instance's type arguments).
    ///
    /// Populated by emit using the enclosing class's `generic_params`.  For
    /// non-generic classes this is a fully-realized template (no `TypeArgRef`).
    pub field_template: crate::TyTemplate,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,
    pub skip: bool,
    /// `@stream.done`: while streaming, the field holds its default until its
    /// value is complete — an incomplete value is never surfaced.
    pub stream_done: bool,
    /// `@stream.must_exist`: the field has no default, so its class has no
    /// partial parse until the field is present.
    pub must_exist: bool,
    /// `@json.rename`: the field's JSON key, under every naming.
    pub json_rename: Option<String>,
    /// `@json.flatten`: the field holds every key no other field took, and
    /// writes them next to the other fields.
    pub json_flatten: bool,
    /// `@json.fallback`: the field holds the whole object, and is all its
    /// class writes.
    pub json_fallback: bool,
    /// `@@json.omit_nulls` on the class, and the field is optional: it is not
    /// written while it holds `null`.
    pub json_omit_null: bool,
    /// `@trace.redact`: telemetry records a redaction in place of the field's
    /// value.
    pub redact: bool,

    /// The exact `type` operand a runtime-constructed field was built from, so
    /// reflection reads back the definitions it carried, not just its shape.
    #[borsh(skip)]
    pub runtime_type: Option<TypeValue>,
}

impl ClassField {
    /// A plain field: `name` of type `ty`, with no attributes.
    pub fn new(name: impl Into<String>, ty: crate::RealizedTy) -> Self {
        Self {
            name: name.into(),
            field_type: RuntimeTy::from(ty.clone()),
            field_template: crate::TyTemplate::from(ty),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip: false,
            stream_done: false,
            must_exist: false,
            json_rename: None,
            json_flatten: false,
            json_fallback: false,
            json_omit_null: false,
            redact: false,
            runtime_type: None,
        }
    }
}

/// Runtime class representation.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Class {
    /// Where this class is declared inside its package: its namespace path
    /// and item name (a class minted through `reflect` has only the item
    /// name). Its own coordinates, and all of its name it carries — see
    /// [`Self::name`]. Never an identity: that is `type_tag` and the
    /// declaration object itself.
    pub path: super::LocalName,

    /// Class fields with type and schema metadata.
    pub fields: Vec<ClassField>,

    /// Class-level description for LLM prompt schema rendering.
    pub description: Option<String>,

    /// Class-level serialization alias.
    pub alias: Option<String>,

    /// Class-level source documentation and custom annotations.
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,

    /// `@@stream.done`: while streaming, an instance is never surfaced until
    /// the whole object is complete.
    pub stream_done: bool,

    /// `@@trace.redact`: telemetry records a redaction in place of every
    /// instance of the class.
    pub redact: bool,

    /// How the compiler decided an instance crosses the host boundary; a
    /// class built through `reflect` is `Fields`. Read through
    /// [`Self::crossing`], which every crossing in either direction asks.
    pub declared_crossing: baml_type::ClassCrossing,

    /// This class's head identity: assigned by the linker (`CLASS_BASE +` the
    /// class's object index in the image) or, for a runtime-created class,
    /// minted fresh by the grafter; in a unit it holds the class's own object
    /// operand. Both the `TypeTag` instruction's jump-table dispatch value and
    /// the identity a `TypeHead` referring to this class compares by.
    pub type_tag: baml_type::typetag::TypeTag,

    /// The class's finalizer (BEP-042), as its position in [`methods`](Self::methods),
    /// or `None` for a class without one. The checker decides which method is
    /// the finalizer; emit records where it placed that method. The position
    /// is validated against the table at load (`Program::validate`), so reading
    /// it needs no name and cannot miss.
    pub cleanup: Option<usize>,

    /// Number of generic params the class itself declares (`GenericBox<T>` ⇒ 1,
    /// non-generic ⇒ 0). A method's `display_type_params` are De Bruijn-ordered
    /// as *class params first, then the method's own*, so this count is the
    /// length of that class prefix — it lets the engine split a method's own
    /// generic params (which Gate A must demand) from the inherited class params
    /// (bound by the receiver, never by name). Set at emit time.
    pub generic_param_count: usize,

    /// What this declaration belongs to: its package (a static package's is
    /// assigned at load), or nothing — a class minted through `reflect`,
    /// which then holds its own witness rules. See [`Owner`](super::Owner).
    #[borsh(skip)]
    pub owner: super::Owner,

    /// The class's inherent methods (static and instance, natives included),
    /// by name, in declaration order. Impl-provided methods are not here:
    /// they are reached through the impl rules. A class created at run time
    /// through `reflect` has an empty table.
    ///
    /// This is the class's method surface by identity — how a package's
    /// export table names a method (`FnPath::Method`), and how a consumer
    /// binds one without a rendered spelling.
    pub methods: IndexMap<baml_type::Name, ClassMethodDef>,

    /// The definition telemetry recorded for this class, made by the first
    /// recorded capture that names it and dropped with the class.
    #[borsh(skip)]
    pub telemetry_definition: btel_types::DefinitionCell,

    /// How this class's instances are laid out, set when the heap allocates
    /// the first one (it registers the layout's descriptor).
    #[borsh(skip)]
    pub layout: std::sync::OnceLock<InstanceLayout>,
}

/// One inherent method of a [`Class`]: the pooled function on the wire, bound
/// to a pointer at load — exactly as an interface's
/// [`InterfaceMethodDef::default`](super::InterfaceMethodDef::default) becomes
/// its `default_fn`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ClassMethodDef {
    /// The method's function object, relocated by the linker like any other
    /// cross-object operand (see `relink::visit_object_operands`).
    pub function: ObjectIndex,
    /// The loaded function: null until the pool is bound. Never serialized —
    /// pointers are runtime-only. A GC edge like `default_fn`.
    #[borsh(skip)]
    pub function_ptr: HeapPtr,
}

impl Class {
    /// How an instance crosses the host boundary. A class with a dynamic
    /// tag (declared at run time, through `reflect` or a runtime-compiled
    /// package) crosses as an opaque reference whatever it declares: no host
    /// has its declaration to build or read one by.
    #[must_use]
    pub fn crossing(&self) -> baml_type::ClassCrossing {
        if self.type_tag.is_dynamic() {
            baml_type::ClassCrossing::Opaque
        } else {
            self.declared_crossing
        }
    }

    /// How this class is named: its path under its package's display name
    /// for a class a package declares (`ai.PromptMessage`), the bare path for
    /// one no package owns (`Person`). Display and boundary spelling only —
    /// see [`declaration_name`](super::declaration_name).
    #[must_use]
    pub fn name(&self) -> crate::DeclarationName {
        self.owner.name_of(&self.path)
    }

    /// This class's finalizer, if it declares one: the method at the recorded
    /// position of its method table. The class is the identity here — nothing
    /// renders a name to find the function.
    #[must_use]
    pub fn cleanup(&self) -> Option<HeapPtr> {
        self.cleanup
            .map(|position| self.methods[position].function_ptr)
    }
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class {}>", self.name())
    }
}
