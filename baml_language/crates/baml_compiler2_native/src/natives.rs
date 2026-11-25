//! The standard library builtins written in Rust, and how compiled code
//! calls the native runtime's implementation of each.
//!
//! The table is generated from the standard library's declarations and the
//! runtime's lists of what it implements, so a builtin the runtime has no
//! implementation of is known here by name and reported as a blocker rather
//! than left for the linker to miss.

use crate::{Blocker, types::NativeTy};

/// A type a native takes or returns.
#[derive(Debug)]
pub(crate) enum Ty {
    Int,
    Float,
    Bool,
    Null,
    String,
    Bigint,
    /// A `uint8array`.
    Bytes,
    Array(&'static Ty),
    /// A map from the builtin's first type parameter to its second.
    Map,
    /// A map from `string` to values of this type.
    MapOf(&'static Ty),
    /// A type or `null`.
    Optional(&'static Ty),
    /// The type a call names for the builtin's type parameter at this
    /// position: passed and returned as the two words a slot of that type
    /// holds it in.
    Elem(usize),
    /// A value of any type: a `json`.
    Any,
    /// An `unknown` parameter of `baml.json`: the id of the description
    /// of the argument's static type, then the value.
    Typed,
    /// The type a call names for the type parameter at this position, as
    /// a value of any type is held: what `baml.json` returns a value it
    /// decoded as.
    BoxedElem(usize),
    /// An instance of a class of the standard library that takes no type
    /// arguments.
    Class(ClassPath),
    /// A `$rust_type` value: a reference to the object holding Rust state.
    Rust,
}

/// A class of the standard library: its namespace under `baml`, and its
/// name.
pub(crate) type ClassPath = (&'static [&'static str], &'static str);

/// The class at `path`, which the standard library the table was made
/// from declares.
pub(crate) fn class_at(
    db: &dyn baml_compiler2_mir::Db,
    (namespace, name): ClassPath,
) -> Option<baml_compiler2_hir_ty::extern_loc::ClassRef<'_>> {
    baml_compiler2_hir_ty::layout::lang_class(db, baml_base::LangPackage::Baml, namespace, name)
}

impl Ty {
    /// Whether a native takes and returns a value of the type as two
    /// words, a tag and a payload: its tag is only known when the program
    /// runs. A value of any other type is one word, with no tag.
    pub(crate) fn is_value(&self) -> bool {
        matches!(
            self,
            Self::Optional(_) | Self::Elem(_) | Self::Any | Self::Typed | Self::BoxedElem(_)
        )
    }
}

impl Ty {
    /// The type at the type arguments `types` of a call.
    pub(crate) fn native<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        types: &[NativeTy<'db>],
    ) -> NativeTy<'db> {
        match self {
            Self::Elem(index) | Self::BoxedElem(index) => types[*index].clone(),
            // Rust state is held as any value is: its tag says what it
            // is.
            Self::Any | Self::Typed | Self::Rust => NativeTy::Unknown,
            Self::Class(path) => class_at(db, *path).map_or(NativeTy::Unknown, |class| {
                NativeTy::Class(class, Vec::new())
            }),
            // `null` is already one of the values of these.
            Self::Optional(Self::Elem(index))
                if matches!(
                    types[*index],
                    NativeTy::Optional(_) | NativeTy::Union(_) | NativeTy::Unknown
                ) =>
            {
                types[*index].clone()
            }
            Self::Int => NativeTy::Int,
            Self::Float => NativeTy::Float,
            Self::Bool => NativeTy::Bool,
            Self::Null => NativeTy::Null,
            Self::String => NativeTy::String,
            Self::Bigint => NativeTy::Bigint,
            Self::Bytes => NativeTy::Bytes,
            Self::Array(element) => NativeTy::Array(Box::new(element.native(db, types))),
            Self::Map => NativeTy::Map(Box::new(types[0].clone()), Box::new(types[1].clone())),
            Self::MapOf(value) => NativeTy::Map(
                Box::new(NativeTy::String),
                Box::new(value.native(db, types)),
            ),
            Self::Optional(inner) => NativeTy::Optional(Box::new(inner.native(db, types))),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Signature {
    /// The receiver, when the builtin is a method, then its parameters.
    pub(crate) params: &'static [Ty],
    pub(crate) result: Ty,
}

/// What the runtime's lists promise of a native, which is what a call of
/// it has to allow for: see the runtime's `natives/README.md`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Flags {
    /// May allocate, and so collect and throw: it takes the worker first.
    /// Without it a native reads its arguments and returns.
    pub(crate) allocates: bool,
    /// May run compiled code and park the calling task before it returns.
    pub(crate) is_async: bool,
}

/// The native `map[key]` is a call of where it is not a place.
pub(crate) const MAP_INDEX: &str = "baml.Map.index";

/// One Rust-declared builtin of the standard library.
#[derive(Debug)]
pub(crate) struct Native {
    /// The builtin's name, as a call of it names it.
    pub(crate) path: &'static str,
    /// The runtime symbol implementing it, when it is ported.
    pub(crate) symbol: &'static str,
    /// `None` when the signature has a type natives cannot take yet.
    pub(crate) signature: Option<Signature>,
    /// Whether the symbol can throw: it then returns two words whatever
    /// its result is, a tag and a payload, and the tag says whether they
    /// are what it threw.
    pub(crate) throws: bool,
    /// `None` when the runtime does not implement it yet.
    pub(crate) flags: Option<Flags>,
    /// How many type parameters the builtin has.
    pub(crate) type_args: usize,
    /// How many type ids a call passes before the declared parameters: one
    /// per type parameter, or none.
    pub(crate) type_ids: usize,
    /// The classes its declaration names in what it takes, returns and
    /// throws: what it can be passed or make an instance of, each of which
    /// a program that calls it describes to the runtime.
    pub(crate) classes: &'static [ClassPath],
}

/// A native is the one at its path.
impl PartialEq for Native {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}
impl Eq for Native {}

include!(concat!(env!("OUT_DIR"), "/native_table.rs"));

/// The Rust-declared builtin called `name`.
pub(crate) fn named(name: &str) -> Option<&'static Native> {
    NATIVES
        .binary_search_by(|native| native.path.cmp(name))
        .ok()
        .map(|index| &NATIVES[index])
}

/// The Rust-declared builtin `function` is. A method of an `implements`
/// block is listed under the key the standard library's build gives it,
/// which is not the name it is displayed by.
pub(crate) fn of<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>,
) -> Option<&'static Native> {
    use baml_compiler2_hir::body::{FunctionBody, function_body};
    named(&baml_compiler2_mir::function_display_name(db, function)).or_else(|| match function {
        baml_compiler2_hir::loc::DeclRef::Source(declared)
            if matches!(
                function_body(db, declared).as_ref(),
                FunctionBody::Builtin(_)
            ) =>
        {
            named(&baml_compiler2_mir::native_key_for(db, declared))
        }
        _ => None,
    })
}

impl Native {
    /// Whether a call of it has the runtime compare two values: `==` on
    /// what no instruction compares, and the `eq` a class has by default.
    pub(crate) fn compares_values(&self) -> bool {
        matches!(
            self.path,
            "baml.ops.equals_equals" | "baml._equals_structural_default"
        )
    }

    /// How to call it, or why compiled code cannot yet.
    pub(crate) fn callable(&self) -> Result<(&Signature, Flags), Blocker> {
        // `map[key]` is compiled as a probe and a panic, and never called:
        // the runtime has no symbol for it.
        let inline = (self.path == MAP_INDEX).then_some(Flags {
            allocates: false,
            is_async: false,
        });
        let Some(flags) = self.flags.or(inline) else {
            return Err(Blocker::new(
                "standard library native that is not ported",
                self.path,
            ));
        };
        match &self.signature {
            Some(signature) => Ok((signature, flags)),
            None => Err(Blocker::new(
                "standard library native with an unsupported signature",
                self.path,
            )),
        }
    }
}
