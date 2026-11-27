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
struct ClassPy {
    cls: Py<PyAny>,
    /// Per field, in slot order, its Python attribute name.
    attrs: Vec<Py<PyString>>,
    /// Every attribute name: what `__pydantic_fields_set__` starts as.
    fields_set: Py<PyFrozenSet>,
}

/// An enum's Python side.
struct EnumPy {
    cls: Py<PyAny>,
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
        }
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
                    let model_fields = cls.getattr(intern!(py, "model_fields"))?;
                    let model_fields = model_fields.cast::<PyDict>()?;
                    let mut attrs = Vec::with_capacity(fields.len());
                    for field in fields {
                        let mut found = None;
                        for (name, info) in model_fields.iter() {
                            let alias = info.getattr(intern!(py, "alias"))?;
                            let matches = if alias.is_none() {
                                name.cast::<PyString>()?.to_str()? == field.name
                            } else {
                                alias.cast::<PyString>()?.to_str()? == field.name
                            };
                            if matches {
                                found = Some(name.cast_into::<PyString>()?);
                                break;
                            }
                        }
                        attrs.push(
                            found
                                .unwrap_or_else(|| PyString::new(py, &field.name))
                                .unbind(),
                        );
                    }
                    let fields_set =
                        PyFrozenSet::new(py, attrs.iter().map(|attr| attr.bind(py)))?.unbind();
                    Ok(Resolved::Class(ClassPy {
                        cls: cls.unbind(),
                        attrs,
                        fields_set,
                    }))
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
                    Ok(Resolved::Enum(EnumPy {
                        cls: cls.unbind(),
                        members,
                        index,
                    }))
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
            Type::List(element) => {
                kind == tag::ARRAY
                    && match value.elements() {
                        Ok(mut elements) => match elements.next() {
                            Some(Ok(first)) => Self::accepts(element, &first),
                            Some(Err(_)) => false,
                            None => true,
                        },
                        Err(_) => false,
                    }
            }
            Type::Map(inner) => {
                kind == tag::MAP
                    && match value.entries() {
                        Ok(entries) => entries
                            .first()
                            .is_none_or(|(_, first)| Self::accepts(inner, first)),
                        Err(_) => false,
                    }
            }
            Type::Union(members) => members.iter().any(|member| Self::accepts(member, value)),
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
                let member = members
                    .iter()
                    .find(|member| Self::accepts(member, value))
                    .ok_or_else(|| {
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
        let cls = class.cls.bind(py);
        match model_mode() {
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

    /// Whether `object` fits `ty`, looking only at the top of a container
    /// and its first element. `widen`: an `int` fits `float`.
    fn fits(&self, py: Python<'_>, ty: &Type, object: &Bound<'_, PyAny>, widen: bool) -> bool {
        let is_bool = object.is_instance_of::<PyBool>();
        let is_int = !is_bool && object.is_instance_of::<PyInt>();
        match ty {
            Type::Null => object.is_none(),
            Type::Bool => is_bool,
            Type::LiteralBool(value) => is_bool && object.is_truthy().is_ok_and(|b| b == *value),
            Type::Int => is_int,
            Type::LiteralInt(value) => is_int && object.extract::<i64>().is_ok_and(|v| v == *value),
            Type::Float => object.is_instance_of::<PyFloat>() || (widen && is_int),
            Type::String => object.is_instance_of::<PyString>(),
            Type::LiteralString(value) => object
                .cast::<PyString>()
                .is_ok_and(|text| text.to_str().is_ok_and(|text| text == value)),
            Type::Class(number) => self
                .class_py(py, *number)
                .is_ok_and(|class| object.is_instance(class.cls.bind(py)).unwrap_or(false)),
            Type::Enum(number) => self
                .enum_py(py, *number)
                .is_ok_and(|e| e.index.contains_key(&(object.as_ptr() as usize))),
            Type::EnumVariant { of, index } => self
                .enum_py(py, *of)
                .is_ok_and(|e| e.index.get(&(object.as_ptr() as usize)) == Some(index)),
            Type::List(element) => {
                let first = if let Ok(list) = object.cast::<PyList>() {
                    Some(list.get_item(0).ok())
                } else if let Ok(tuple) = object.cast::<PyTuple>() {
                    Some(tuple.get_item(0).ok())
                } else {
                    None
                };
                match first {
                    None => false,
                    Some(None) => true,
                    Some(Some(first)) => self.fits(py, element, &first, widen),
                }
            }
            Type::Map(inner) => match object.cast::<PyDict>() {
                Ok(dict) => dict
                    .values()
                    .get_item(0)
                    .map_or(true, |first| self.fits(py, inner, &first, widen)),
                Err(_) => false,
            },
            Type::Union(members) => members.iter().any(|m| self.fits(py, m, object, widen)),
        }
    }

    /// The member of a union `object` is built as.
    fn pick<'t>(
        &self,
        py: Python<'_>,
        ty: &Type,
        members: &'t [Type],
        object: &Bound<'_, PyAny>,
    ) -> PyResult<&'t Type> {
        for widen in [false, true] {
            let mut fitting = members.iter().filter(|m| self.fits(py, m, object, widen));
            let Some(first) = fitting.next() else {
                continue;
            };
            // An empty container fits every container member equally: the
            // value does not say which it is.
            let empty = object.len().is_ok_and(|length| length == 0)
                && matches!(first, Type::List(_) | Type::Map(_));
            if empty && fitting.any(|other| other != first) {
                return Err(PyTypeError::new_err(format!(
                    "an empty {} fits more than one member of `{}`",
                    if matches!(first, Type::List(_)) {
                        "list"
                    } else {
                        "dict"
                    },
                    self.type_name(ty)
                )));
            }
            return Ok(first);
        }
        Err(PyTypeError::new_err(format!(
            "{} does not fit `{}`",
            describe(object),
            self.type_name(ty)
        )))
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
            return Err(PyTypeError::new_err(format!(
                "an argument nested too deeply: more than {MAX_DEPTH} levels"
            )));
        }
        let mismatch = || {
            PyTypeError::new_err(format!(
                "{} does not fit `{}`",
                describe(object),
                self.type_name(ty)
            ))
        };
        let refused = |error: SdkError| PyTypeError::new_err(error.to_string());
        let is_bool = object.is_instance_of::<PyBool>();
        match ty {
            Type::Int | Type::LiteralInt(_) => {
                if is_bool || !object.is_instance_of::<PyInt>() {
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
            Type::Float => {
                if let Ok(float) = object.cast::<PyFloat>() {
                    Ok(session.float(float.value()))
                } else if !is_bool && object.is_instance_of::<PyInt>() {
                    // Today's SDK widens an int given for a float.
                    let value: f64 = object.extract().map_err(|_| mismatch())?;
                    Ok(session.float(value))
                } else {
                    Err(mismatch())
                }
            }
            Type::Bool | Type::LiteralBool(_) => {
                if !is_bool {
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
                let text = object
                    .cast::<PyString>()
                    .map_err(|_| mismatch())?
                    .to_str()?;
                if let Type::LiteralString(literal) = ty
                    && text != literal
                {
                    return Err(mismatch());
                }
                session.string(text).map_err(refused)
            }
            Type::Class(number) => {
                let class = self.class_py(py, *number)?;
                if !object.is_instance(class.cls.bind(py))? {
                    return Err(mismatch());
                }
                let TypeDefKind::Class { fields } = &self.def(*number)?.kind else {
                    return Err(mismatch());
                };
                let dict = object.getattr(intern!(py, "__dict__"))?;
                let dict = dict.cast::<PyDict>()?;
                let mut values = Vec::with_capacity(fields.len());
                for (field, attr) in fields.iter().zip(&class.attrs) {
                    let attr = attr.bind(py);
                    let item = match dict.get_item(attr)? {
                        Some(item) => item,
                        None => object.getattr(attr)?,
                    };
                    values.push(self.build(py, session, &field.ty, &item, depth + 1)?);
                }
                session.instance(*number, values).map_err(refused)
            }
            Type::Enum(number) | Type::EnumVariant { of: number, .. } => {
                let enumeration = self.enum_py(py, *number)?;
                let index = match enumeration.index.get(&(object.as_ptr() as usize)) {
                    Some(index) => *index,
                    None => {
                        // A member of an equal enum, or its value.
                        let member = enumeration
                            .cls
                            .bind(py)
                            .call1((object,))
                            .map_err(|_| mismatch())?;
                        *enumeration
                            .index
                            .get(&(member.as_ptr() as usize))
                            .ok_or_else(mismatch)?
                    }
                };
                if let Type::EnumVariant { index: wanted, .. } = ty
                    && index != *wanted
                {
                    return Err(mismatch());
                }
                session.variant(*number, index).map_err(refused)
            }
            Type::List(element) => {
                let items: Vec<Bound<'_, PyAny>> = if let Ok(list) = object.cast::<PyList>() {
                    list.iter().collect()
                } else if let Ok(tuple) = object.cast::<PyTuple>() {
                    tuple.iter().collect()
                } else {
                    return Err(mismatch());
                };
                let mut values = Vec::with_capacity(items.len());
                for item in &items {
                    values.push(self.build(py, session, element, item, depth + 1)?);
                }
                session.array(values).map_err(refused)
            }
            Type::Map(inner) => {
                let dict = object.cast::<PyDict>().map_err(|_| mismatch())?;
                let mut entries = Vec::with_capacity(dict.len());
                for (key, item) in dict.iter() {
                    let key = key
                        .cast::<PyString>()
                        .map_err(|_| PyTypeError::new_err("a map's keys must be strings"))?
                        .to_str()?
                        .to_owned();
                    let key = session.string(&key).map_err(refused)?;
                    entries.push((key, self.build(py, session, inner, &item, depth + 1)?));
                }
                session.map(entries).map_err(refused)
            }
            Type::Union(members) => {
                let member = self.pick(py, ty, members, object)?;
                self.build(py, session, member, object, depth)
            }
        }
    }
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
