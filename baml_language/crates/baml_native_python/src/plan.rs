//! Copying values between BAML memory and Python objects, by the types of
//! the library's interface.
//!
//! A result is copied in one native call: the walk reads the BAML value
//! through the core's reads (`field`, `elements`, `entries`,
//! `variant_index`, `class_of`) and builds the Python object directly. An
//! argument goes the other way, by the parameter's declared type. Each
//! class and enum finds its Python class through the generated type map
//! the first time it is used.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU8, Ordering},
};

use baml_native_host::{BamlValue, Interface, SdkError, Session, Type, TypeDef, TypeDefKind, tag};
use pyo3::{
    exceptions::{PyRuntimeError, PyTypeError},
    intern,
    prelude::*,
    sync::PyOnceLock,
    types::{PyBool, PyDict, PyFloat, PyFrozenSet, PyInt, PyList, PySet, PyString, PyTuple},
};

/// How deep a value may nest, either way. A walk deeper than this would
/// risk the stack of a thread that is not the main one.
const MAX_DEPTH: u32 = 200;

/// How a model is made from a result's fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ModelMode {
    /// `cls.model_validate(fields)`, as today's SDK does.
    Validate = 0,
    /// `cls.model_construct(**fields)`.
    Construct = 1,
    /// What `model_construct` does, done from here: `object.__new__(cls)`,
    /// then `__dict__`, `__pydantic_fields_set__`, `__pydantic_extra__`,
    /// `__pydantic_private__`.
    Native = 2,
}

static MODEL_MODE: AtomicU8 = AtomicU8::new(ModelMode::Native as u8);

pub(crate) fn set_model_mode(mode: ModelMode) {
    MODEL_MODE.store(mode as u8, Ordering::Relaxed);
}

fn model_mode() -> ModelMode {
    match MODEL_MODE.load(Ordering::Relaxed) {
        0 => ModelMode::Validate,
        1 => ModelMode::Construct,
        _ => ModelMode::Native,
    }
}

/// A class's Python side.
pub(crate) struct ClassPy {
    cls: Py<PyAny>,
    /// Per field, in slot order, its Python attribute name.
    attrs: Vec<Py<PyString>>,
    /// Every attribute name: what `__pydantic_fields_set__` starts as.
    fields_set: Py<PyFrozenSet>,
    /// The model is one the native construct would build wrong: it allows
    /// extra fields, or has private attributes, validators or a
    /// `model_post_init`. It is always made with `model_validate`.
    validate_always: bool,
}

impl ClassPy {
    /// The Python side of `cls`, a pydantic model, for fields named (as BAML
    /// names them) `fields`, in slot order. A field is matched to its
    /// attribute by alias, then by name.
    pub(crate) fn of(py: Python<'_>, cls: Bound<'_, PyAny>, fields: &[&str]) -> PyResult<Self> {
        let model_fields = cls.getattr(intern!(py, "model_fields"))?;
        let entries = model_fields.cast::<PyDict>()?.items();
        let mut attrs = Vec::with_capacity(fields.len());
        for field in fields {
            let mut found = None;
            for entry in entries.iter() {
                let (name, info) = item_pair(&entry)?;
                let alias = info.getattr(intern!(py, "alias"))?;
                let named = if alias.is_none() { &name } else { &alias };
                if named.cast::<PyString>()?.to_str()? == *field {
                    found = Some(name.cast_into::<PyString>()?);
                    break;
                }
            }
            attrs.push(found.unwrap_or_else(|| PyString::new(py, field)).unbind());
        }
        let fields_set = PyFrozenSet::new(py, attrs.iter().map(|attr| attr.bind(py)))?.unbind();
        let validate_always = needs_validation(py, &cls);
        Ok(Self {
            cls: cls.unbind(),
            attrs,
            fields_set,
            validate_always,
        })
    }
}

impl ClassPy {
    pub(crate) fn attr<'py>(&self, py: Python<'py>, slot: usize) -> &Bound<'py, PyString> {
        self.attrs[slot].bind(py)
    }

    pub(crate) fn validates_always(&self) -> bool {
        self.validate_always
    }
}

fn item_pair<'py>(item: &Bound<'py, PyAny>) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    item.extract()
}

/// Whether the native construct would build `cls` wrong: `extra` allowed,
/// private attributes, validators, or a `model_post_init` of its own.
fn needs_validation(py: Python<'_>, cls: &Bound<'_, PyAny>) -> bool {
    let not_empty = |object: PyResult<Bound<'_, PyAny>>| {
        object.is_ok_and(|object| object.len().is_ok_and(|n| n > 0))
    };
    let extra = cls
        .getattr(intern!(py, "model_config"))
        .and_then(|config| config.get_item("extra"))
        .is_ok_and(|extra| {
            extra
                .extract::<String>()
                .is_ok_and(|extra| extra == "allow")
        });
    let private = not_empty(cls.getattr(intern!(py, "__private_attributes__")));
    let post_init = cls
        .getattr(intern!(py, "__pydantic_post_init__"))
        .is_ok_and(|hook| !hook.is_none());
    let validators = cls
        .getattr(intern!(py, "__pydantic_decorators__"))
        .is_ok_and(|decorators| {
            [
                "validators",
                "field_validators",
                "root_validators",
                "model_validators",
            ]
            .iter()
            .any(|kind| not_empty(decorators.getattr(*kind)))
        });
    extra || private || post_init || validators
}

/// An enum's Python side.
struct EnumPy {
    /// Per variant, in order, its member.
    members: Vec<Py<PyAny>>,
    /// A member's address to its index.
    index: HashMap<usize, u32>,
}

enum Resolved {
    Class(ClassPy),
    Enum(EnumPy),
}

/// What turns interface types into Python objects and back.
pub(crate) struct Plans {
    interface: &'static Interface,
    resolved: Vec<PyOnceLock<Resolved>>,
    type_map: PyOnceLock<Py<PyAny>>,
    object_new: PyOnceLock<Py<PyAny>>,
    enum_type: PyOnceLock<Py<PyAny>>,
}

/// A value of an unexpected shape: an error of the bindings.
pub(crate) fn decode(error: &SdkError) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

impl Plans {
    pub(crate) fn new(interface: &'static Interface) -> Self {
        Self {
            interface,
            resolved: interface.types.iter().map(|_| PyOnceLock::new()).collect(),
            type_map: PyOnceLock::new(),
            object_new: PyOnceLock::new(),
            enum_type: PyOnceLock::new(),
        }
    }

    /// The generated class named `fqn` in the type map, if there is one.
    pub(crate) fn class_of_name<'py>(
        &self,
        py: Python<'py>,
        fqn: &str,
    ) -> Option<Bound<'py, PyAny>> {
        self.type_map
            .get(py)?
            .bind(py)
            .call_method1(intern!(py, "get_class"), (fqn,))
            .ok()
            .filter(|cls| !cls.is_none())
    }

    pub(crate) fn interface(&self) -> &'static Interface {
        self.interface
    }

    pub(crate) fn set_type_map(&self, py: Python<'_>, type_map: Py<PyAny>) {
        let _ = self.type_map.set(py, type_map);
    }

    fn def(&self, number: u32) -> PyResult<&'static TypeDef> {
        self.interface
            .types
            .get(number as usize)
            .ok_or_else(|| PyRuntimeError::new_err(format!("no type numbered {number}")))
    }

    /// The type's name as Python's type map has it.
    pub(crate) fn fqn(&self, number: u32) -> String {
        self.interface
            .types
            .get(number as usize)
            .map_or_else(|| format!("#{number}"), |def| format!("user.{}", def.name))
    }

    fn resolve(&self, py: Python<'_>, number: u32) -> PyResult<&Resolved> {
        let slot = self
            .resolved
            .get(number as usize)
            .ok_or_else(|| PyRuntimeError::new_err(format!("no type numbered {number}")))?;
        slot.get_or_try_init(py, || {
            let def = self.def(number)?;
            let type_map = self.type_map.get(py).ok_or_else(|| {
                PyRuntimeError::new_err("the type map is not set: call baml_native.set_type_map")
            })?;
            let type_map = type_map.bind(py);
            let fqn = format!("user.{}", def.name);
            match &def.kind {
                TypeDefKind::Class { fields } => {
                    let cls = type_map.call_method1(intern!(py, "get_class"), (&fqn,))?;
                    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
                    ClassPy::of(py, cls, &names).map(Resolved::Class)
                }
                TypeDefKind::Enum { variants } => {
                    let cls = type_map.call_method1(intern!(py, "get_enum"), (&fqn,))?;
                    let mut members = Vec::with_capacity(variants.len());
                    let mut index = HashMap::new();
                    for (place, variant) in variants.iter().enumerate() {
                        let member = match cls.call1((variant,)) {
                            Ok(member) => member,
                            Err(_) => cls.get_item(variant)?,
                        };
                        index.insert(
                            member.as_ptr() as usize,
                            u32::try_from(place).unwrap_or(u32::MAX),
                        );
                        members.push(member.unbind());
                    }
                    Ok(Resolved::Enum(EnumPy { members, index }))
                }
            }
        })
    }

    fn class_py(&self, py: Python<'_>, number: u32) -> PyResult<&ClassPy> {
        match self.resolve(py, number)? {
            Resolved::Class(class) => Ok(class),
            Resolved::Enum(_) => Err(PyRuntimeError::new_err(format!(
                "{} is an enum",
                self.fqn(number)
            ))),
        }
    }

    fn enum_py(&self, py: Python<'_>, number: u32) -> PyResult<&EnumPy> {
        match self.resolve(py, number)? {
            Resolved::Enum(enumeration) => Ok(enumeration),
            Resolved::Class(_) => Err(PyRuntimeError::new_err(format!(
                "{} is a class",
                self.fqn(number)
            ))),
        }
    }

    /// A type as text, for errors.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "int".to_owned(),
            Type::Float => "float".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Null => "null".to_owned(),
            Type::String => "string".to_owned(),
            Type::LiteralInt(value) => value.to_string(),
            Type::LiteralBool(value) => value.to_string(),
            Type::LiteralString(value) => format!("{value:?}"),
            Type::Class(number) | Type::Enum(number) => self.fqn(*number),
            Type::EnumVariant { of, index } => {
                let variant = match self.interface.types.get(*of as usize).map(|def| &def.kind) {
                    Some(TypeDefKind::Enum { variants }) => {
                        variants.get(*index as usize).cloned().unwrap_or_default()
                    }
                    _ => String::new(),
                };
                format!("{}.{variant}", self.fqn(*of))
            }
            Type::List(element) => format!("{}[]", self.type_name(element)),
            Type::Map(value) => format!("map<string, {}>", self.type_name(value)),
            Type::Union(members) => members
                .iter()
                .map(|member| self.type_name(member))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    // ---- out of BAML ----

    /// Whether `value` can be read as `ty`: what picks a union's member.
    /// A list or a map is told apart by its first element.
    fn accepts(ty: &Type, value: &BamlValue<'_>) -> bool {
        let kind = value.kind();
        match ty {
            Type::Null => value.is_null(),
            Type::Int | Type::LiteralInt(_) => value.raw().tag == tag::INT,
            Type::Float => value.raw().tag == tag::FLOAT,
            Type::Bool | Type::LiteralBool(_) => value.raw().tag == tag::BOOL,
            Type::String | Type::LiteralString(_) => value.is_string(),
            Type::Class(number) => kind == tag::INSTANCE && value.class_of() == Some(*number),
            Type::Enum(number) | Type::EnumVariant { of: number, .. } => {
                kind == tag::ENUM_VARIANT && value.class_of() == Some(*number)
            }
            // Every element: an array does not say its element type.
            Type::List(element) => {
                kind == tag::ARRAY
                    && value.elements().is_ok_and(|mut elements| {
                        elements.all(|item| item.is_ok_and(|item| Self::accepts(element, &item)))
                    })
            }
            Type::Map(inner) => {
                kind == tag::MAP
                    && value.entries().is_ok_and(|entries| {
                        entries.iter().all(|(_, item)| Self::accepts(inner, item))
                    })
            }
            Type::Union(members) => members.iter().any(|member| Self::accepts(member, value)),
        }
    }

    /// Whether `value`'s tag alone can be `ty`: no element is read.
    fn tag_fits(ty: &Type, value: &BamlValue<'_>) -> bool {
        let kind = value.kind();
        match ty {
            Type::Null => value.is_null(),
            Type::Int | Type::LiteralInt(_) => value.raw().tag == tag::INT,
            Type::Float => value.raw().tag == tag::FLOAT,
            Type::Bool | Type::LiteralBool(_) => value.raw().tag == tag::BOOL,
            Type::String | Type::LiteralString(_) => value.is_string(),
            Type::Class(_) => kind == tag::INSTANCE,
            Type::Enum(_) | Type::EnumVariant { .. } => kind == tag::ENUM_VARIANT,
            Type::List(_) => kind == tag::ARRAY,
            Type::Map(_) => kind == tag::MAP,
            Type::Union(members) => members.iter().any(|member| Self::tag_fits(member, value)),
        }
    }

    /// Copy `value`, of type `ty`, into a Python object.
    pub(crate) fn to_py<'py>(
        &self,
        py: Python<'py>,
        ty: &Type,
        value: &BamlValue<'_>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.out(py, ty, value, 0)
    }

    fn out<'py>(
        &self,
        py: Python<'py>,
        ty: &Type,
        value: &BamlValue<'_>,
        depth: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        if depth > MAX_DEPTH {
            return Err(PyRuntimeError::new_err(format!(
                "a result nested too deeply: more than {MAX_DEPTH} levels"
            )));
        }
        match ty {
            Type::Int | Type::LiteralInt(_) => Ok(value
                .int()
                .map_err(|e| decode(&e))?
                .into_pyobject(py)?
                .into_any()),
            Type::Float => Ok(value
                .float()
                .map_err(|e| decode(&e))?
                .into_pyobject(py)?
                .into_any()),
            Type::Bool | Type::LiteralBool(_) => {
                Ok(PyBool::new(py, value.bool().map_err(|e| decode(&e))?)
                    .to_owned()
                    .into_any())
            }
            Type::Null => Ok(py.None().into_bound(py)),
            Type::String | Type::LiteralString(_) => {
                Ok(PyString::new(py, &value.string().map_err(|e| decode(&e))?).into_any())
            }
            Type::Class(number) => self.class_out(py, *number, value, depth),
            Type::Enum(number) | Type::EnumVariant { of: number, .. } => {
                let enumeration = self.enum_py(py, *number)?;
                let index = value.variant_index().map_err(|e| decode(&e))?;
                enumeration
                    .members
                    .get(index as usize)
                    .map(|member| member.bind(py).clone())
                    .ok_or_else(|| {
                        PyRuntimeError::new_err(format!(
                            "no variant {index} of {}",
                            self.fqn(*number)
                        ))
                    })
            }
            Type::List(element) => {
                let elements = value.elements().map_err(|e| decode(&e))?;
                let mut items = Vec::new();
                for item in elements {
                    items.push(self.out(py, element, &item.map_err(|e| decode(&e))?, depth + 1)?);
                }
                Ok(PyList::new(py, items)?.into_any())
            }
            Type::Map(inner) => {
                let dict = PyDict::new(py);
                for (key, item) in value.entries().map_err(|e| decode(&e))? {
                    let key = PyString::new(py, &key.string().map_err(|e| decode(&e))?);
                    dict.set_item(key, self.out(py, inner, &item, depth + 1)?)?;
                }
                Ok(dict.into_any())
            }
            Type::Union(members) => {
                // By the value's tag when one member alone has it (the usual
                // case: `T | null`); otherwise by reading it.
                let mut by_tag = members
                    .iter()
                    .filter(|member| Self::tag_fits(member, value));
                let member = match (by_tag.next(), by_tag.next()) {
                    (Some(only), None) => Some(only),
                    _ => members.iter().find(|member| Self::accepts(member, value)),
                };
                let member = member.ok_or_else(|| {
                    PyRuntimeError::new_err(format!(
                        "a value of tag {} is no member of {}",
                        value.kind(),
                        self.type_name(ty)
                    ))
                })?;
                self.out(py, member, value, depth)
            }
        }
    }

    fn class_out<'py>(
        &self,
        py: Python<'py>,
        number: u32,
        value: &BamlValue<'_>,
        depth: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let class = self.class_py(py, number)?;
        let TypeDefKind::Class { fields } = &self.def(number)?.kind else {
            return Err(PyRuntimeError::new_err("not a class"));
        };
        value.expect_class(number).map_err(|e| decode(&e))?;
        let dict = PyDict::new(py);
        for (slot, (field, attr)) in fields.iter().zip(&class.attrs).enumerate() {
            let item = value
                .field(u32::try_from(slot).unwrap_or(u32::MAX))
                .map_err(|e| decode(&e))?;
            dict.set_item(attr.bind(py), self.out(py, &field.ty, &item, depth + 1)?)?;
        }
        self.make_model(py, class, dict)
    }

    /// A model of `class` from its fields, keyed by attribute name, the way
    /// the model mode says.
    pub(crate) fn make_model<'py>(
        &self,
        py: Python<'py>,
        class: &ClassPy,
        dict: Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let cls = class.cls.bind(py);
        let mode = match model_mode() {
            ModelMode::Native if class.validate_always => ModelMode::Validate,
            mode => mode,
        };
        match mode {
            ModelMode::Validate => cls.call_method1(intern!(py, "model_validate"), (dict,)),
            ModelMode::Construct => {
                cls.call_method(intern!(py, "model_construct"), (), Some(&dict))
            }
            ModelMode::Native => {
                let object_new = self.object_new.get_or_try_init(py, || {
                    py.import("builtins")?
                        .getattr("object")?
                        .getattr("__new__")
                        .map(Bound::unbind)
                })?;
                let instance = object_new.bind(py).call1((cls,))?;
                set_slot(&instance, intern!(py, "__dict__"), dict.as_any())?;
                let fields_set = PySet::new(py, class.fields_set.bind(py).iter())?;
                set_slot(
                    &instance,
                    intern!(py, "__pydantic_fields_set__"),
                    fields_set.as_any(),
                )?;
                let none = py.None().into_bound(py);
                set_slot(&instance, intern!(py, "__pydantic_extra__"), &none)?;
                set_slot(&instance, intern!(py, "__pydantic_private__"), &none)?;
                Ok(instance)
            }
        }
    }

    // ---- into BAML ----
    //
    // Arguments are read as today's SDK's encoder reads them
    // (`bridge_encode`, `Rules::Python`): `bool` is not an `int`; an `int`
    // fits a `float` only when the float is exactly that integer; an enum
    // member is a variant of its own enum only, and is told apart before
    // `str`, which generated enums mix in; a `str` names a variant by
    // conversion; a pydantic instance is an instance of its class, and a
    // `dict` with string keys stands in for a class when every key is a
    // declared field (a field left out is null where the class allows
    // null). Under a union the members are ranked by how the value's shape
    // stands to them (identity, then natural, then conversion). Among the
    // best rank whose members take the whole value, a single type wins;
    // otherwise the value's own type; otherwise the member that left out
    // the fewest fields; otherwise the value is ambiguous.

    fn is_enum_member(&self, py: Python<'_>, object: &Bound<'_, PyAny>) -> bool {
        if object.is_none()
            || object.is_exact_instance_of::<PyString>()
            || object.is_exact_instance_of::<PyInt>()
            || object.is_exact_instance_of::<PyBool>()
            || object.is_exact_instance_of::<PyFloat>()
            || object.is_exact_instance_of::<PyList>()
            || object.is_exact_instance_of::<PyDict>()
        {
            return false;
        }
        self.enum_type
            .get_or_try_init(py, || py.import("enum")?.getattr("Enum").map(Bound::unbind))
            .is_ok_and(|enumeration| object.is_instance(enumeration.bind(py)).unwrap_or(false))
    }

    fn shape(&self, py: Python<'_>, object: &Bound<'_, PyAny>) -> Shape {
        if object.is_none() {
            Shape::Null
        } else if self.is_enum_member(py, object) {
            Shape::Member
        } else if object.is_instance_of::<PyBool>() {
            Shape::Bool
        } else if object.is_instance_of::<PyInt>() {
            Shape::Int
        } else if object.is_instance_of::<PyFloat>() {
            Shape::Float
        } else if object.is_instance_of::<PyString>() {
            Shape::Str
        } else if object.is_instance_of::<PyList>() || object.is_instance_of::<PyTuple>() {
            Shape::Seq
        } else if object.is_instance_of::<PyDict>() {
            Shape::Dict
        } else {
            Shape::Object
        }
    }

    /// How `object`'s shape stands to `ty`, or `None` when it never can.
    fn rank(
        &self,
        py: Python<'_>,
        ty: &Type,
        object: &Bound<'_, PyAny>,
        shape: Shape,
    ) -> Option<Rank> {
        match (shape, ty) {
            (_, Type::Union(members)) => members
                .iter()
                .filter_map(|member| self.rank(py, member, object, shape))
                .min(),
            (Shape::Null, Type::Null)
            | (Shape::Bool, Type::Bool | Type::LiteralBool(_))
            | (Shape::Int, Type::Int | Type::LiteralInt(_))
            | (Shape::Float, Type::Float)
            | (Shape::Str, Type::String | Type::LiteralString(_)) => Some(Rank::Identity),
            (Shape::Member, Type::Enum(number) | Type::EnumVariant { of: number, .. }) => self
                .enum_py(py, *number)
                .ok()
                .filter(|e| e.index.contains_key(&(object.as_ptr() as usize)))
                .map(|_| Rank::Identity),
            (Shape::Int, Type::Float) => exact_float(object).map(|_| Rank::Conversion),
            (Shape::Str, Type::Enum(_) | Type::EnumVariant { .. }) => Some(Rank::Conversion),
            (Shape::Seq, Type::List(_)) | (Shape::Dict, Type::Map(_)) => Some(Rank::Natural),
            (Shape::Dict, Type::Class(_)) => Some(Rank::Conversion),
            (Shape::Object, Type::Class(number)) => self
                .class_py(py, *number)
                .ok()
                .filter(|class| object.is_instance(class.cls.bind(py)).unwrap_or(false))
                .map(|_| Rank::Identity),
            _ => None,
        }
    }

    /// The index of the variant of enum `number` that `object` is: one of
    /// its members, or a string naming one.
    fn variant_of(
        &self,
        py: Python<'_>,
        number: u32,
        object: &Bound<'_, PyAny>,
        shape: Shape,
    ) -> Option<u32> {
        match shape {
            Shape::Member => self
                .enum_py(py, number)
                .ok()?
                .index
                .get(&(object.as_ptr() as usize))
                .copied(),
            Shape::Str => {
                let text = object.cast::<PyString>().ok()?.to_str().ok()?;
                let TypeDefKind::Enum { variants } = &self.def(number).ok()?.kind else {
                    return None;
                };
                variants
                    .iter()
                    .position(|variant| variant == text)
                    .and_then(|place| u32::try_from(place).ok())
            }
            _ => None,
        }
    }

    /// The value of field `slot` of class `class` from `source`, if given.
    fn field_value<'py>(
        py: Python<'py>,
        class: &ClassPy,
        name: &str,
        slot: usize,
        source: &Source<'py>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        match source {
            Source::Instance { object, dict } => {
                let attr = class.attrs[slot].bind(py);
                match dict.get_item(attr)? {
                    Some(value) => Ok(Some(value)),
                    None if object.hasattr(attr)? => object.getattr(attr).map(Some),
                    None => Ok(None),
                }
            }
            Source::Mapping(dict) => dict.get_item(name),
        }
    }

    /// Where a class argument's fields come from: an instance of the class,
    /// or a `dict` keyed by field name whose every key is a field.
    fn source<'py>(
        &self,
        py: Python<'py>,
        number: u32,
        object: &Bound<'py, PyAny>,
        shape: Shape,
    ) -> PyResult<Option<Source<'py>>> {
        let class = self.class_py(py, number)?;
        match shape {
            Shape::Object if object.is_instance(class.cls.bind(py))? => {
                let dict = object
                    .getattr(intern!(py, "__dict__"))?
                    .cast_into::<PyDict>()?;
                Ok(Some(Source::Instance {
                    object: object.clone(),
                    dict,
                }))
            }
            Shape::Dict => {
                let dict = object.clone().cast_into::<PyDict>()?;
                let TypeDefKind::Class { fields } = &self.def(number)?.kind else {
                    return Ok(None);
                };
                for key in dict.keys().iter() {
                    let Ok(key) = key.cast::<PyString>() else {
                        return Ok(None);
                    };
                    let key = key.to_str()?;
                    if !fields.iter().any(|field| field.name == key) {
                        return Ok(None);
                    }
                }
                Ok(Some(Source::Mapping(dict)))
            }
            _ => Ok(None),
        }
    }

    /// Whether `object` takes `ty` all the way down: `Some` with how many
    /// fields it left out, `None` when it does not fit.
    fn check(
        &self,
        py: Python<'_>,
        ty: &Type,
        object: &Bound<'_, PyAny>,
        depth: u32,
    ) -> PyResult<Option<u32>> {
        if depth > MAX_DEPTH {
            return Err(too_deep());
        }
        let shape = self.shape(py, object);
        let fits = |fit: bool| Ok(fit.then_some(0));
        match ty {
            Type::Union(members) => Ok(match self.choose(py, members, object, depth, false)? {
                Choice::One(_, omitted) => Some(omitted),
                Choice::Unfit | Choice::Ambiguous => None,
            }),
            Type::Null => fits(shape == Shape::Null),
            Type::Bool => fits(shape == Shape::Bool),
            Type::LiteralBool(value) => fits(shape == Shape::Bool && object.is_truthy()? == *value),
            Type::Int => fits(shape == Shape::Int && object.extract::<i64>().is_ok()),
            Type::LiteralInt(value) => {
                fits(shape == Shape::Int && object.extract::<i64>().is_ok_and(|v| v == *value))
            }
            Type::Float => fits(
                shape == Shape::Float || (shape == Shape::Int && exact_float(object).is_some()),
            ),
            Type::String => fits(shape == Shape::Str),
            Type::LiteralString(value) => {
                fits(shape == Shape::Str && object.cast::<PyString>()?.to_str()? == value)
            }
            Type::Enum(number) => fits(self.variant_of(py, *number, object, shape).is_some()),
            Type::EnumVariant { of, index } => {
                fits(self.variant_of(py, *of, object, shape) == Some(*index))
            }
            Type::List(element) => {
                if shape != Shape::Seq {
                    return Ok(None);
                }
                let mut omitted = 0;
                for item in object.try_iter()? {
                    match self.check(py, element, &item?, depth + 1)? {
                        Some(count) => omitted += count,
                        None => return Ok(None),
                    }
                }
                Ok(Some(omitted))
            }
            Type::Map(inner) => {
                if shape != Shape::Dict {
                    return Ok(None);
                }
                let mut omitted = 0;
                for item in object.cast::<PyDict>()?.items().iter() {
                    let (key, value) = item_pair(&item)?;
                    if !key.is_instance_of::<PyString>() {
                        return Ok(None);
                    }
                    match self.check(py, inner, &value, depth + 1)? {
                        Some(count) => omitted += count,
                        None => return Ok(None),
                    }
                }
                Ok(Some(omitted))
            }
            Type::Class(number) => {
                let Some(source) = self.source(py, *number, object, shape)? else {
                    return Ok(None);
                };
                let class = self.class_py(py, *number)?;
                let TypeDefKind::Class { fields } = &self.def(*number)?.kind else {
                    return Ok(None);
                };
                let mut omitted = 0;
                for (slot, field) in fields.iter().enumerate() {
                    match Self::field_value(py, class, &field.name, slot, &source)? {
                        Some(value) => match self.check(py, &field.ty, &value, depth + 1)? {
                            Some(count) => omitted += count,
                            None => return Ok(None),
                        },
                        None if nullable(&field.ty) => omitted += 1,
                        None => return Ok(None),
                    }
                }
                Ok(Some(omitted))
            }
        }
    }

    /// The member of a union `object` is built as.
    fn choose<'t>(
        &self,
        py: Python<'_>,
        members: &'t [Type],
        object: &Bound<'_, PyAny>,
        depth: u32,
        building: bool,
    ) -> PyResult<Choice<'t>> {
        let shape = self.shape(py, object);
        let mut ranked: Vec<(Rank, &'t Type)> = members
            .iter()
            .filter_map(|member| {
                self.rank(py, member, object, shape)
                    .map(|rank| (rank, member))
            })
            .collect();
        // When building, a member the shape alone picks is built at once: a
        // value that does not fit it fails the build with the same error.
        if building && let [(_, only)] = ranked.as_slice() {
            return Ok(Choice::One(only, 0));
        }
        ranked.sort_by_key(|(rank, _)| *rank);
        let mut index = 0;
        while index < ranked.len() {
            let rank = ranked[index].0;
            let mut fits: Vec<(&'t Type, u32)> = Vec::new();
            while index < ranked.len() && ranked[index].0 == rank {
                let member = ranked[index].1;
                index += 1;
                if let Some(omitted) = self.check(py, member, object, depth)? {
                    if !fits.iter().any(|(seen, _)| equivalent(seen, member)) {
                        fits.push((member, omitted));
                    }
                }
            }
            match fits.as_slice() {
                [] => continue,
                [(member, omitted)] => return Ok(Choice::One(member, *omitted)),
                _ => {}
            }
            if let Some((member, omitted)) = fits
                .iter()
                .find(|(member, _)| self.is_natural(py, member, object))
            {
                return Ok(Choice::One(member, *omitted));
            }
            let fewest = fits.iter().map(|(_, omitted)| *omitted).min().unwrap_or(0);
            let mut cheapest = fits.iter().filter(|(_, omitted)| *omitted == fewest);
            return Ok(match (cheapest.next(), cheapest.next()) {
                (Some((member, omitted)), None) => Choice::One(member, *omitted),
                _ => Choice::Ambiguous,
            });
        }
        Ok(Choice::Unfit)
    }

    /// Whether `ty` is the type `object` has by itself, as the compiler
    /// would type it as a literal.
    fn is_natural(&self, py: Python<'_>, ty: &Type, object: &Bound<'_, PyAny>) -> bool {
        let shape = self.shape(py, object);
        match (ty, shape) {
            (Type::Null, Shape::Null)
            | (Type::Bool, Shape::Bool)
            | (Type::Int, Shape::Int)
            | (Type::Float, Shape::Float)
            | (Type::String, Shape::Str) => true,
            (Type::Enum(number), Shape::Member) => {
                self.variant_of(py, *number, object, shape).is_some()
            }
            (Type::Class(_), Shape::Object) => {
                self.rank(py, ty, object, shape) == Some(Rank::Identity)
            }
            (Type::List(element), Shape::Seq) => object
                .try_iter()
                .and_then(Iterator::collect::<PyResult<Vec<_>>>)
                .is_ok_and(|items| self.all_natural(py, element, &items)),
            (Type::Map(inner), Shape::Dict) => object.cast::<PyDict>().is_ok_and(|dict| {
                self.all_natural(py, inner, &dict.values().iter().collect::<Vec<_>>())
            }),
            _ => false,
        }
    }

    /// Whether `ty` is the join of the natural types of `items`.
    fn all_natural(&self, py: Python<'_>, ty: &Type, items: &[Bound<'_, PyAny>]) -> bool {
        if items.is_empty() {
            return false;
        }
        match ty {
            Type::Union(members) => {
                items
                    .iter()
                    .all(|item| members.iter().any(|m| self.is_natural(py, m, item)))
                    && members
                        .iter()
                        .all(|m| items.iter().any(|item| self.is_natural(py, m, item)))
            }
            ty => items.iter().all(|item| self.is_natural(py, ty, item)),
        }
    }

    /// Build `object` in BAML memory as a value of `ty`.
    pub(crate) fn to_baml<'s>(
        &self,
        py: Python<'_>,
        session: &'s Session,
        ty: &Type,
        object: &Bound<'_, PyAny>,
    ) -> PyResult<BamlValue<'s>> {
        self.build(py, session, ty, object, 0)
    }

    fn build<'s>(
        &self,
        py: Python<'_>,
        session: &'s Session,
        ty: &Type,
        object: &Bound<'_, PyAny>,
        depth: u32,
    ) -> PyResult<BamlValue<'s>> {
        if depth > MAX_DEPTH {
            return Err(too_deep());
        }
        let mismatch = || {
            PyTypeError::new_err(format!(
                "{} does not fit `{}`",
                describe(object),
                self.type_name(ty)
            ))
        };
        let refused = |error: SdkError| PyTypeError::new_err(error.to_string());
        // Exact built-in types are told at once; anything else is looked at
        // as today's encoder does (an enum member before what it mixes in).
        let shape = || self.shape(py, object);
        match ty {
            Type::Union(members) => match self.choose(py, members, object, depth, true)? {
                Choice::One(member, _) => self.build(py, session, member, object, depth),
                Choice::Unfit => Err(mismatch()),
                Choice::Ambiguous => Err(PyTypeError::new_err(format!(
                    "{} fits more than one member of `{}`: say which",
                    describe(object),
                    self.type_name(ty)
                ))),
            },
            Type::Int | Type::LiteralInt(_) => {
                if !object.is_exact_instance_of::<PyInt>() && shape() != Shape::Int {
                    return Err(mismatch());
                }
                let value: i64 = object.extract().map_err(|_| mismatch())?;
                if let Type::LiteralInt(literal) = ty
                    && value != *literal
                {
                    return Err(mismatch());
                }
                Ok(session.int(value))
            }
            Type::Float => match if object.is_exact_instance_of::<PyFloat>() {
                Shape::Float
            } else {
                shape()
            } {
                Shape::Float => Ok(session.float(object.cast::<PyFloat>()?.value())),
                // Today's SDK widens an int only when the float is exactly it.
                Shape::Int => exact_float(object)
                    .map(|value| session.float(value))
                    .ok_or_else(mismatch),
                _ => Err(mismatch()),
            },
            Type::Bool | Type::LiteralBool(_) => {
                if !object.is_instance_of::<PyBool>() {
                    return Err(mismatch());
                }
                let value = object.is_truthy()?;
                if let Type::LiteralBool(literal) = ty
                    && value != *literal
                {
                    return Err(mismatch());
                }
                Ok(session.bool(value))
            }
            Type::Null => {
                if object.is_none() {
                    Ok(session.null())
                } else {
                    Err(mismatch())
                }
            }
            Type::String | Type::LiteralString(_) => {
                if !object.is_exact_instance_of::<PyString>() && shape() != Shape::Str {
                    return Err(mismatch());
                }
                let text = object.cast::<PyString>()?.to_str()?;
                if let Type::LiteralString(literal) = ty
                    && text != literal
                {
                    return Err(mismatch());
                }
                session.string(text).map_err(refused)
            }
            Type::Enum(number) | Type::EnumVariant { of: number, .. } => {
                let index = self
                    .variant_of(py, *number, object, shape())
                    .ok_or_else(mismatch)?;
                if let Type::EnumVariant { index: wanted, .. } = ty
                    && index != *wanted
                {
                    return Err(mismatch());
                }
                session.variant(*number, index).map_err(refused)
            }
            Type::List(element) => {
                if !object.is_exact_instance_of::<PyList>() && shape() != Shape::Seq {
                    return Err(mismatch());
                }
                let mut values = Vec::with_capacity(object.len().unwrap_or(0));
                if let Ok(list) = object.cast::<PyList>() {
                    for item in list.iter() {
                        values.push(self.build(py, session, element, &item, depth + 1)?);
                    }
                } else {
                    for item in object.try_iter()? {
                        values.push(self.build(py, session, element, &item?, depth + 1)?);
                    }
                }
                session.array(values).map_err(refused)
            }
            Type::Map(inner) => {
                if !object.is_exact_instance_of::<PyDict>() && shape() != Shape::Dict {
                    return Err(mismatch());
                }
                // A snapshot of the entries, as today's encoder walks
                // `items()`: code run while a value is read may change the
                // dict.
                let items = object.cast::<PyDict>()?.items();
                let mut entries = Vec::with_capacity(items.len());
                for item in items.iter() {
                    let (key, value) = item_pair(&item)?;
                    let key = key
                        .cast::<PyString>()
                        .map_err(|_| PyTypeError::new_err("a map's keys must be strings"))?
                        .to_str()?
                        .to_owned();
                    let key = session.string(&key).map_err(refused)?;
                    entries.push((key, self.build(py, session, inner, &value, depth + 1)?));
                }
                session.map(entries).map_err(refused)
            }
            Type::Class(number) => {
                let class = self.class_py(py, *number)?;
                // An instance of the class, the usual case, first.
                let source = if object.is_instance(class.cls.bind(py))? {
                    Source::Instance {
                        object: object.clone(),
                        dict: object
                            .getattr(intern!(py, "__dict__"))?
                            .cast_into::<PyDict>()?,
                    }
                } else {
                    self.source(py, *number, object, shape())?
                        .ok_or_else(mismatch)?
                };
                let TypeDefKind::Class { fields } = &self.def(*number)?.kind else {
                    return Err(mismatch());
                };
                let mut values = Vec::with_capacity(fields.len());
                for (slot, field) in fields.iter().enumerate() {
                    values.push(
                        match Self::field_value(py, class, &field.name, slot, &source)? {
                            Some(value) => self.build(py, session, &field.ty, &value, depth + 1)?,
                            None if nullable(&field.ty) => session.null(),
                            None => {
                                return Err(PyTypeError::new_err(format!(
                                    "{} has no field {:?} of `{}`",
                                    describe(object),
                                    field.name,
                                    self.type_name(ty)
                                )));
                            }
                        },
                    );
                }
                session.instance(*number, values).map_err(refused)
            }
        }
    }
}

/// What a Python value is, one level down, for the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Null,
    /// A member of an `enum.Enum`, whatever it mixes in.
    Member,
    Bool,
    Int,
    Float,
    Str,
    /// A `list` or a `tuple`.
    Seq,
    Dict,
    Object,
}

/// How a value's shape stands to a union member, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Identity,
    Natural,
    Conversion,
}

enum Choice<'t> {
    One(&'t Type, u32),
    Unfit,
    Ambiguous,
}

enum Source<'py> {
    Instance {
        object: Bound<'py, PyAny>,
        dict: Bound<'py, PyDict>,
    },
    Mapping(Bound<'py, PyDict>),
}

fn too_deep() -> PyErr {
    PyTypeError::new_err(format!(
        "an argument nested too deeply: more than {MAX_DEPTH} levels"
    ))
}

/// Whether `ty` admits `null`.
fn nullable(ty: &Type) -> bool {
    match ty {
        Type::Null => true,
        Type::Union(members) => members.iter().any(nullable),
        _ => false,
    }
}

/// The same type, unions compared as sets.
fn equivalent(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Union(left), Type::Union(right)) => {
            left.iter().all(|l| right.iter().any(|r| equivalent(l, r)))
                && right.iter().all(|r| left.iter().any(|l| equivalent(l, r)))
        }
        (Type::List(left), Type::List(right)) | (Type::Map(left), Type::Map(right)) => {
            equivalent(left, right)
        }
        _ => a == b,
    }
}

/// A Python `int` as the `float` that is exactly it, if there is one.
fn exact_float(object: &Bound<'_, PyAny>) -> Option<f64> {
    if let Ok(int) = object.extract::<i64>() {
        #[expect(clippy::cast_precision_loss, reason = "checked to be exact just below")]
        let float = int as f64;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a finite float back to an integer"
        )]
        let back = float as i128;
        return (back == i128::from(int)).then_some(float);
    }
    let float: f64 = object.extract().ok()?;
    object.eq(float).ok()?.then_some(float)
}

/// Set an attribute as `object.__setattr__` does, skipping the class's own
/// `__setattr__` (pydantic's, which validates).
fn set_slot(
    object: &Bound<'_, PyAny>,
    name: &Bound<'_, PyString>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    // SAFETY: three live objects; `PyObject_GenericSetAttr` borrows them.
    #[expect(
        unsafe_code,
        reason = "the generic setter, as object.__setattr__ calls it"
    )]
    let status = unsafe {
        pyo3::ffi::PyObject_GenericSetAttr(object.as_ptr(), name.as_ptr(), value.as_ptr())
    };
    if status == 0 {
        Ok(())
    } else {
        Err(PyErr::fetch(object.py()))
    }
}

/// A Python object for an error message: its type and a short repr.
fn describe(object: &Bound<'_, PyAny>) -> String {
    let type_name = object
        .get_type()
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "?".to_owned());
    let repr = object
        .repr()
        .map(|repr| repr.to_string())
        .unwrap_or_default();
    let repr: String = repr.chars().take(60).collect();
    format!("{type_name} {repr}")
}
