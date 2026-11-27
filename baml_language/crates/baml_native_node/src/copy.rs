//! Plans: how each exported type is copied between BAML memory and
//! JavaScript.
//!
//! At `bind` every exported type becomes a plan, from the interface's typed
//! tree. A result is copied by walking the BAML value with the core's reads
//! (`field`, `elements`, `entries`, ...) and building the JavaScript value
//! directly; an argument is built the other way, by the parameter's
//! declared type, bottom-up, each part a handle until it is built into its
//! parent. Layout stays the library's: nothing here reads BAML memory.
//!
//! What a result becomes is what today's TypeScript SDK returns: an `int`
//! is a number, a class an instance of its generated class (a plain
//! null-prototype object when it has none), an enum value its generated
//! enum's member, a map a null-prototype object.

use std::collections::HashMap;

use baml_native_host::{BamlValue, Interface, Library, SdkError, Session, Type, TypeDefKind, tag};
use napi_sys::ValueType;

use crate::js::{self, Env, JsResult, Ref, Thrown, Value};

/// How a class instance becomes a JavaScript object (`bind`'s `mode`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Build {
    /// `new C()`, then each field set from native code.
    SetProperties,
    /// One call per object into a per-class factory taking the field values
    /// in order, which sets them on `new C()` in JavaScript.
    PositionalFactory,
    /// One call per object into a per-class factory doing what today's
    /// bridge does: `new C({f0: v0, f1: v1, ..})`.
    InitFactory,
}

/// How a value of a declared type is copied out.
enum Out {
    Int,
    Float,
    Bool,
    Null,
    String,
    Class(u32),
    Enum(u32),
    List(Box<Out>),
    Map(Box<Out>),
    /// By the value's own tag: unions, literals, optionals.
    Dynamic,
}

impl Out {
    fn of(ty: &Type) -> Self {
        match ty {
            Type::Int => Self::Int,
            Type::Float => Self::Float,
            Type::Bool => Self::Bool,
            Type::Null => Self::Null,
            Type::String => Self::String,
            Type::Class(class) => Self::Class(*class),
            Type::Enum(of) | Type::EnumVariant { of, .. } => Self::Enum(*of),
            Type::List(element) => Self::List(Box::new(Self::of(element))),
            Type::Map(value) => Self::Map(Box::new(Self::of(value))),
            Type::LiteralInt(_)
            | Type::LiteralBool(_)
            | Type::LiteralString(_)
            | Type::Union(_) => Self::Dynamic,
        }
    }
}

struct FieldPlan {
    name: String,
    /// The field's name as a JavaScript string, kept alive.
    key: Ref,
    ty: Type,
    out: Out,
    nullable: bool,
}

enum TypePlan {
    Class {
        name: String,
        fields: Vec<FieldPlan>,
        /// The generated class, if the type map has it.
        constructor: Option<Ref>,
        /// The per-class factory of the build mode, if it uses one.
        factory: Option<Ref>,
    },
    Enum {
        name: String,
        variants: Vec<String>,
        /// What each variant is in JavaScript: the generated enum's member.
        values: Vec<Ref>,
        index: HashMap<String, u32>,
    },
}

struct FunctionPlan {
    name: String,
    params: Vec<(String, Type, bool)>,
    result: Out,
}

pub(crate) struct Plans {
    functions: Vec<FunctionPlan>,
    types: Vec<TypePlan>,
    /// `() => Object.create(null)`.
    make_map: Ref,
}

/// Why a copy out failed.
pub(crate) enum CopyError {
    /// JavaScript threw (a constructor): what it threw.
    Js(Value),
    /// The value did not have the shape its type says.
    Decode(String),
}

impl From<Thrown> for CopyError {
    fn from(thrown: Thrown) -> Self {
        Self::Js(thrown.0)
    }
}

impl From<SdkError> for CopyError {
    fn from(error: SdkError) -> Self {
        Self::Decode(error.to_string())
    }
}

/// Why an argument could not be built.
enum ArgError {
    Js(Value),
    /// Where, and why, as in ``argument `x`.inner: expected int, found a string``.
    Unfit(String),
}

impl From<Thrown> for ArgError {
    fn from(thrown: Thrown) -> Self {
        Self::Js(thrown.0)
    }
}

fn admits_null(ty: &Type) -> bool {
    match ty {
        Type::Null => true,
        Type::Union(members) => members.iter().any(admits_null),
        _ => false,
    }
}

/// The name of an interface type as the typemap and today's errors spell
/// it: user code lives under `user.`.
fn user_name(name: &str) -> String {
    format!("user.{name}")
}

/// `types()`: per interface type, `[name, isEnum, fieldOrVariantNames]`.
pub(crate) fn describe_types(env: Env, library: &Library) -> Value {
    let types = &library.interface().types;
    let out = js::array(env, types.len());
    for (index, def) in types.iter().enumerate() {
        let (is_enum, names): (bool, Vec<&str>) = match &def.kind {
            TypeDefKind::Class { fields } => (
                false,
                fields.iter().map(|field| field.name.as_str()).collect(),
            ),
            TypeDefKind::Enum { variants } => (true, variants.iter().map(String::as_str).collect()),
        };
        let list = js::array(env, names.len());
        for (i, name) in names.iter().enumerate() {
            let _ = js::set_element(env, list, js::index(i), js::string(env, name));
        }
        let row = js::array(env, 3);
        let _ = js::set_element(env, row, 0, js::string(env, &def.name));
        let _ = js::set_element(env, row, 1, js::boolean(env, is_enum));
        let _ = js::set_element(env, row, 2, list);
        let _ = js::set_element(env, out, js::index(index), row);
    }
    out
}

/// `functions()`: per export, `[name, paramNames, hasDefault]`.
pub(crate) fn describe_functions(env: Env, library: &Library) -> Value {
    let functions = &library.interface().functions;
    let out = js::array(env, functions.len());
    for (index, function) in functions.iter().enumerate() {
        let names = js::array(env, function.params.len());
        let defaults = js::array(env, function.params.len());
        for (i, param) in function.params.iter().enumerate() {
            let _ = js::set_element(env, names, js::index(i), js::string(env, &param.name));
            let _ = js::set_element(
                env,
                defaults,
                js::index(i),
                js::boolean(env, param.has_default),
            );
        }
        let row = js::array(env, 3);
        let _ = js::set_element(env, row, 0, js::string(env, &function.name));
        let _ = js::set_element(env, row, 1, names);
        let _ = js::set_element(env, row, 2, defaults);
        let _ = js::set_element(env, out, js::index(index), row);
    }
    out
}

fn optional_ref(env: Env, array: Value, index: u32) -> JsResult<Option<Ref>> {
    if js::type_of(env, array) != ValueType::napi_object {
        return Ok(None);
    }
    let value = js::get_element(env, array, index)?;
    Ok(match js::type_of(env, value) {
        ValueType::napi_undefined | ValueType::napi_null => None,
        _ => Some(js::reference(env, value)),
    })
}

impl Plans {
    /// Plans for every export and every type of `library`'s interface.
    /// `constructors[n]`, `factories[n]` and `enum_values[n]` are what
    /// JavaScript has for interface type `n`.
    pub(crate) fn build(
        env: Env,
        library: &Library,
        constructors: Value,
        factories: Value,
        enum_values: Value,
        make_map: Value,
        mode: u64,
    ) -> JsResult<Self> {
        let interface: &Interface = library.interface();
        let build = match mode {
            1 => Build::PositionalFactory,
            2 => Build::InitFactory,
            _ => Build::SetProperties,
        };
        let mut types = Vec::with_capacity(interface.types.len());
        for (number, def) in interface.types.iter().enumerate() {
            let number = js::index(number);
            types.push(match &def.kind {
                TypeDefKind::Class { fields } => TypePlan::Class {
                    name: def.name.clone(),
                    fields: fields
                        .iter()
                        .map(|field| FieldPlan {
                            name: field.name.clone(),
                            key: js::reference(env, js::string(env, &field.name)),
                            ty: field.ty.clone(),
                            out: Out::of(&field.ty),
                            nullable: admits_null(&field.ty),
                        })
                        .collect(),
                    constructor: optional_ref(env, constructors, number)?,
                    factory: if build == Build::SetProperties {
                        None
                    } else {
                        optional_ref(env, factories, number)?
                    },
                },
                TypeDefKind::Enum { variants } => {
                    let given = if js::type_of(env, enum_values) == ValueType::napi_object {
                        Some(js::get_element(env, enum_values, number)?)
                    } else {
                        None
                    };
                    let mut values = Vec::with_capacity(variants.len());
                    for (i, variant) in variants.iter().enumerate() {
                        let value = match given {
                            Some(list) if js::type_of(env, list) == ValueType::napi_object => {
                                let value = js::get_element(env, list, js::index(i))?;
                                if js::type_of(env, value) == ValueType::napi_undefined {
                                    js::string(env, variant)
                                } else {
                                    value
                                }
                            }
                            _ => js::string(env, variant),
                        };
                        values.push(js::reference(env, value));
                    }
                    TypePlan::Enum {
                        name: def.name.clone(),
                        variants: variants.clone(),
                        values,
                        index: variants
                            .iter()
                            .enumerate()
                            .map(|(i, variant)| (variant.clone(), js::index(i)))
                            .collect(),
                    }
                }
            });
        }
        let functions = interface
            .functions
            .iter()
            .map(|function| FunctionPlan {
                name: function.name.clone(),
                params: function
                    .params
                    .iter()
                    .map(|param| (param.name.clone(), param.ty.clone(), param.has_default))
                    .collect(),
                result: Out::of(&function.result),
            })
            .collect();
        Ok(Self {
            functions,
            types,
            make_map: js::reference(env, make_map),
        })
    }

    // ---- results ---------------------------------------------------------

    /// The result of export `export`, copied into JavaScript.
    pub(crate) fn result(
        &self,
        env: Env,
        export: u32,
        value: &BamlValue<'_>,
    ) -> Result<Value, CopyError> {
        let out = self
            .functions
            .get(export as usize)
            .map_or(&Out::Dynamic, |function| &function.result);
        self.copy(env, out, value)
    }

    fn copy(&self, env: Env, out: &Out, value: &BamlValue<'_>) -> Result<Value, CopyError> {
        Ok(match out {
            Out::Int => int_value(env, value)?,
            Out::Float => js::number(env, value.float()?),
            Out::Bool => js::boolean(env, value.bool()?),
            Out::Null => js::null(env),
            Out::String => js::string(env, &value.string()?),
            Out::Class(class) => self.copy_class(env, *class, value)?,
            Out::Enum(of) => self.copy_enum(env, *of, value)?,
            Out::List(element_out) => {
                let elements = value.elements()?;
                let (length, _) = elements.size_hint();
                let array = js::array(env, length);
                for (i, element) in elements.enumerate() {
                    let element = self.copy(env, element_out, &element?)?;
                    js::set_element(env, array, js::index(i), element)?;
                }
                array
            }
            Out::Map(entry) => {
                let map = js::call(env, js::deref(env, self.make_map), &[])?;
                for (key, item) in value.entries()? {
                    let key = js::string(env, &key.string()?);
                    let item = self.copy(env, entry, &item)?;
                    js::set_property(env, map, key, item)?;
                }
                map
            }
            Out::Dynamic => self.copy_dynamic(env, value)?,
        })
    }

    /// A value copied by its own tag.
    fn copy_dynamic(&self, env: Env, value: &BamlValue<'_>) -> Result<Value, CopyError> {
        let raw = value.raw();
        if raw.tag & tag::HANDLE == 0 {
            return Ok(match raw.tag {
                tag::NULL => js::null(env),
                tag::BOOL => js::boolean(env, value.bool()?),
                tag::INT => int_value(env, value)?,
                tag::FLOAT => js::number(env, value.float()?),
                other => return Err(CopyError::Decode(format!("a scalar of tag {other}"))),
            });
        }
        Ok(match value.kind() {
            tag::STRING | tag::STRING_SLICE => js::string(env, &value.string()?),
            tag::INSTANCE => match value.class_of() {
                Some(class) => self.copy_class(env, class, value)?,
                // An instance of a class the interface does not describe
                // (the standard library's): its text.
                None => js::string(env, &value.describe()),
            },
            tag::ENUM_VARIANT => match value.class_of() {
                Some(of) => self.copy_enum(env, of, value)?,
                None => js::string(env, &value.describe()),
            },
            tag::ARRAY => self.copy(env, &Out::List(Box::new(Out::Dynamic)), value)?,
            tag::MAP => self.copy(env, &Out::Map(Box::new(Out::Dynamic)), value)?,
            other => return Err(CopyError::Decode(format!("an object of tag {other}"))),
        })
    }

    fn copy_class(&self, env: Env, class: u32, value: &BamlValue<'_>) -> Result<Value, CopyError> {
        let Some(TypePlan::Class {
            fields,
            constructor,
            factory,
            ..
        }) = self.types.get(class as usize)
        else {
            return Err(CopyError::Decode(format!("type {class} is not a class")));
        };
        // Up to sixteen fields are kept on the stack.
        let mut few = [std::ptr::null_mut(); 16];
        let mut many = Vec::new();
        let values: &mut [Value] = if fields.len() <= few.len() {
            &mut few[..fields.len()]
        } else {
            many.resize(fields.len(), std::ptr::null_mut());
            &mut many
        };
        for (index, field) in fields.iter().enumerate() {
            let item = value.field(js::index(index))?;
            values[index] = self.copy(env, &field.out, &item)?;
        }
        if let (Some(factory), Some(_)) = (factory, constructor) {
            return Ok(js::call(env, js::deref(env, *factory), values)?);
        }
        let object = match constructor {
            Some(constructor) => js::new_instance(env, js::deref(env, *constructor), &[])?,
            None => js::call(env, js::deref(env, self.make_map), &[])?,
        };
        for (field, item) in fields.iter().zip(values.iter()) {
            js::set_property(env, object, js::deref(env, field.key), *item)?;
        }
        Ok(object)
    }

    fn copy_enum(&self, env: Env, of: u32, value: &BamlValue<'_>) -> Result<Value, CopyError> {
        let Some(TypePlan::Enum { values, .. }) = self.types.get(of as usize) else {
            return Err(CopyError::Decode(format!("type {of} is not an enum")));
        };
        let index = value.variant_index()?;
        values
            .get(index as usize)
            .map(|variant| js::deref(env, *variant))
            .ok_or_else(|| CopyError::Decode(format!("no variant {index} of enum {of}")))
    }

    /// What a call threw or panicked with, for JavaScript to build its error
    /// from: `{ value, className, text }`. `value` is the thrown value copied
    /// by its tag (a declared class arrives as its generated class);
    /// `className` its type's name when the interface has it, `text` the
    /// value as an uncaught throw of it is written.
    pub(crate) fn thrown(&self, env: Env, value: &BamlValue<'_>) -> Value {
        let object = js::object(env);
        let copied = match self.copy_dynamic(env, value) {
            Ok(copied) => copied,
            Err(CopyError::Js(_) | CopyError::Decode(_)) => js::undefined(env),
        };
        let class_name = value.class_of().and_then(|number| {
            self.types.get(number as usize).map(|def| match def {
                TypePlan::Class { name, .. } | TypePlan::Enum { name, .. } => user_name(name),
            })
        });
        let text = if value.raw().tag & tag::HANDLE == 0 {
            String::new()
        } else {
            value.describe()
        };
        let _ = js::set_property(env, object, js::string(env, "value"), copied);
        let class_name =
            class_name.map_or_else(|| js::undefined(env), |name| js::string(env, &name));
        let _ = js::set_property(env, object, js::string(env, "className"), class_name);
        let _ = js::set_property(env, object, js::string(env, "text"), js::string(env, &text));
        object
    }

    // ---- arguments -------------------------------------------------------

    /// The arguments of export `export`, built from the JavaScript array
    /// `args` (in parameter order; `undefined` for one not given). Throws
    /// `invalid_argument` errors as today's SDK words them.
    pub(crate) fn arguments<'s>(
        &self,
        env: Env,
        session: &'s Session,
        export: u32,
        args: Value,
    ) -> Result<Vec<BamlValue<'s>>, Value> {
        // An export the interface does not have gets no arguments: the
        // library refuses it, with its reason.
        let Some(function) = self.functions.get(export as usize) else {
            return Ok(Vec::new());
        };
        if !js::is_array(env, args) {
            return Err(js::error(env, None, "the arguments must be an array"));
        }
        let given = js::array_length(env, args);
        let mut built = Vec::with_capacity(function.params.len());
        let mut buffer = Vec::new();
        for (index, (name, ty, has_default)) in function.params.iter().enumerate() {
            let arg = if js::index(index) < given {
                js::get_element(env, args, js::index(index)).map_err(|thrown| thrown.0)?
            } else {
                js::undefined(env)
            };
            if *has_default && js::type_of(env, arg) == ValueType::napi_undefined {
                built.push(session.omitted());
                continue;
            }
            let mut path = format!("argument `{name}`");
            match self.arg(env, session, ty, arg, &mut path, &mut buffer) {
                Ok(value) => built.push(value),
                Err(ArgError::Js(thrown)) => return Err(thrown),
                Err(ArgError::Unfit(why)) => {
                    return Err(js::error(env, Some("invalid_argument"), &why));
                }
            }
        }
        let _ = &function.name;
        Ok(built)
    }

    fn render(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "int".to_owned(),
            Type::Float => "float".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Null => "null".to_owned(),
            Type::String => "string".to_owned(),
            Type::LiteralInt(value) => value.to_string(),
            Type::LiteralBool(value) => value.to_string(),
            Type::LiteralString(value) => format!("{value:?}"),
            Type::Class(number) | Type::Enum(number) => self.type_name(*number),
            Type::EnumVariant { of, index } => match self.types.get(*of as usize) {
                Some(TypePlan::Enum { name, variants, .. }) => format!(
                    "{}.{}",
                    user_name(name),
                    variants.get(*index as usize).map_or("?", String::as_str)
                ),
                _ => self.type_name(*of),
            },
            Type::List(element) => format!("{}[]", self.render(element)),
            Type::Map(value) => format!("map<string, {}>", self.render(value)),
            Type::Union(members) => members
                .iter()
                .map(|member| self.render(member))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    fn type_name(&self, number: u32) -> String {
        match self.types.get(number as usize) {
            Some(TypePlan::Class { name, .. } | TypePlan::Enum { name, .. }) => user_name(name),
            None => format!("type {number}"),
        }
    }

    fn unfit(&self, env: Env, ty: &Type, value: Value, path: &str) -> ArgError {
        ArgError::Unfit(format!(
            "{path}: expected {}, found {}",
            self.render(ty),
            found(env, value)
        ))
    }

    #[expect(clippy::too_many_lines, reason = "one arm per type")]
    fn arg<'s>(
        &self,
        env: Env,
        session: &'s Session,
        ty: &Type,
        value: Value,
        path: &mut String,
        buffer: &mut Vec<u8>,
    ) -> Result<BamlValue<'s>, ArgError> {
        let kind = js::type_of(env, value);
        match ty {
            Type::Int => {
                if kind == ValueType::napi_number {
                    let n = js::get_f64(env, value);
                    if n.fract() == 0.0 && n.abs() <= 9.223_372_036_854_775e18 {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "a whole number within the range of an int: exact"
                        )]
                        return Ok(session.int(n as i64));
                    }
                }
                Err(self.unfit(env, ty, value, path))
            }
            Type::Float => {
                if kind == ValueType::napi_number {
                    return Ok(session.float(js::get_f64(env, value)));
                }
                Err(self.unfit(env, ty, value, path))
            }
            Type::Bool => {
                if kind == ValueType::napi_boolean {
                    return Ok(session.bool(js::get_bool(env, value)));
                }
                Err(self.unfit(env, ty, value, path))
            }
            Type::Null => {
                if matches!(kind, ValueType::napi_null | ValueType::napi_undefined) {
                    return Ok(session.null());
                }
                Err(self.unfit(env, ty, value, path))
            }
            Type::String => {
                if kind == ValueType::napi_string {
                    let text = js::get_string(env, value, buffer);
                    return session.string(text).map_err(|error| refused(path, &error));
                }
                Err(self.unfit(env, ty, value, path))
            }
            Type::LiteralString(expected) => {
                if kind == ValueType::napi_string && js::get_string(env, value, buffer) == expected
                {
                    return session
                        .string(expected)
                        .map_err(|error| refused(path, &error));
                }
                Err(ArgError::Unfit(format!(
                    "{path}: {} does not fit the literal type {}",
                    found(env, value),
                    self.render(ty)
                )))
            }
            Type::LiteralInt(expected) => {
                if kind == ValueType::napi_number {
                    let n = js::get_f64(env, value);
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "compared only when whole and within the range of an int"
                    )]
                    if n.fract() == 0.0
                        && n.abs() <= 9.223_372_036_854_775e18
                        && n as i64 == *expected
                    {
                        return Ok(session.int(*expected));
                    }
                }
                Err(ArgError::Unfit(format!(
                    "{path}: {} does not fit the literal type {expected}",
                    found(env, value)
                )))
            }
            Type::LiteralBool(expected) => {
                if kind == ValueType::napi_boolean && js::get_bool(env, value) == *expected {
                    return Ok(session.bool(*expected));
                }
                Err(ArgError::Unfit(format!(
                    "{path}: {} does not fit the literal type {expected}",
                    found(env, value)
                )))
            }
            Type::Enum(of) | Type::EnumVariant { of, .. } => {
                let Some(TypePlan::Enum {
                    name,
                    variants,
                    index,
                    ..
                }) = self.types.get(*of as usize)
                else {
                    return Err(ArgError::Unfit(format!("{path}: type {of} is not an enum")));
                };
                if kind != ValueType::napi_string {
                    return Err(self.unfit(env, ty, value, path));
                }
                let text = js::get_string(env, value, buffer);
                let Some(&position) = index.get(text) else {
                    return Err(ArgError::Unfit(format!(
                        "{path}: enum {} has no variant `{text}`",
                        user_name(name)
                    )));
                };
                if let Type::EnumVariant { index: wanted, .. } = ty
                    && position != *wanted
                {
                    return Err(ArgError::Unfit(format!(
                        "{path}: {} does not fit the literal type {}",
                        found(env, value),
                        self.render(ty)
                    )));
                }
                let _ = variants;
                session
                    .variant(*of, position)
                    .map_err(|error| refused(path, &error))
            }
            Type::List(element) => {
                if kind != ValueType::napi_object || !js::is_array(env, value) {
                    return Err(self.unfit(env, ty, value, path));
                }
                let length = js::array_length(env, value);
                let mut elements = Vec::with_capacity(length as usize);
                for i in 0..length {
                    let item = js::get_element(env, value, i)?;
                    let built = self.arg(env, session, element, item, path, buffer);
                    elements.push(self.placed(
                        env,
                        session,
                        element,
                        item,
                        path,
                        buffer,
                        built,
                        || format!("[{i}]"),
                    )?);
                }
                session
                    .array(elements)
                    .map_err(|error| refused(path, &error))
            }
            Type::Map(item_type) => {
                if kind != ValueType::napi_object || js::is_array(env, value) {
                    return Err(self.unfit(env, ty, value, path));
                }
                let keys = js::keys(env, value)?;
                let length = js::array_length(env, keys);
                let mut entries = Vec::with_capacity(length as usize);
                for i in 0..length {
                    let key = js::get_element(env, keys, i)?;
                    let item = js::get_property(env, value, key)?;
                    let text = js::get_string(env, key, buffer).to_owned();
                    let built = self.arg(env, session, item_type, item, path, buffer);
                    let item =
                        self.placed(env, session, item_type, item, path, buffer, built, || {
                            format!("[{text:?}]")
                        })?;
                    entries.push((
                        session
                            .string(&text)
                            .map_err(|error| refused(path, &error))?,
                        item,
                    ));
                }
                session.map(entries).map_err(|error| refused(path, &error))
            }
            Type::Class(class) => self.arg_class(env, session, *class, ty, value, path, buffer),
            Type::Union(members) => self.arg_union(env, session, ty, members, value, path, buffer),
        }
    }

    /// An element or entry built with the container's path: when it is
    /// refused, it is walked again with its place added, for the message.
    /// The place is only formatted then, not for every element.
    #[expect(clippy::too_many_arguments, reason = "the walk's state")]
    fn placed<'s>(
        &self,
        env: Env,
        session: &'s Session,
        ty: &Type,
        value: Value,
        path: &mut String,
        buffer: &mut Vec<u8>,
        built: Result<BamlValue<'s>, ArgError>,
        place: impl FnOnce() -> String,
    ) -> Result<BamlValue<'s>, ArgError> {
        match built {
            Err(ArgError::Unfit(why)) => {
                let base = path.len();
                path.push_str(&place());
                let again = self.arg(env, session, ty, value, path, buffer);
                path.truncate(base);
                Err(again.err().unwrap_or(ArgError::Unfit(why)))
            }
            other => other,
        }
    }

    #[expect(clippy::too_many_arguments, reason = "the walk's state")]
    fn arg_class<'s>(
        &self,
        env: Env,
        session: &'s Session,
        class: u32,
        ty: &Type,
        value: Value,
        path: &mut String,
        buffer: &mut Vec<u8>,
    ) -> Result<BamlValue<'s>, ArgError> {
        let Some(TypePlan::Class { name, fields, .. }) = self.types.get(class as usize) else {
            return Err(ArgError::Unfit(format!(
                "{path}: type {class} is not a class"
            )));
        };
        if js::type_of(env, value) != ValueType::napi_object || js::is_array(env, value) {
            return Err(self.unfit(env, ty, value, path));
        }
        let mut built = Vec::with_capacity(fields.len());
        let base = path.len();
        for field in fields {
            let item = js::get_property(env, value, js::deref(env, field.key))?;
            if js::type_of(env, item) == ValueType::napi_undefined {
                if field.nullable {
                    built.push(session.null());
                    continue;
                }
                return Err(ArgError::Unfit(format!(
                    "{path}: class {} requires field `{}`",
                    user_name(name),
                    field.name
                )));
            }
            path.push('.');
            path.push_str(&field.name);
            built.push(self.arg(env, session, &field.ty, item, path, buffer)?);
            path.truncate(base);
        }
        session
            .instance(class, built)
            .map_err(|error| ArgError::Unfit(format!("{path}: {error}")))
    }

    /// A member of a union, chosen by the JavaScript value: `null` for a
    /// nullable union, a number as `int` when it is whole and `int` is a
    /// member (else `float`), a string as `string`, a literal or an enum
    /// variant it names, an instance of a generated class as that class,
    /// and anything else by trying each member that could take it, in
    /// order.
    #[expect(clippy::too_many_arguments, reason = "the walk's state")]
    fn arg_union<'s>(
        &self,
        env: Env,
        session: &'s Session,
        ty: &Type,
        members: &[Type],
        value: Value,
        path: &mut String,
        buffer: &mut Vec<u8>,
    ) -> Result<BamlValue<'s>, ArgError> {
        let kind = js::type_of(env, value);
        let candidates: Vec<&Type> = match kind {
            ValueType::napi_undefined | ValueType::napi_null => members
                .iter()
                .filter(|member| admits_null(member))
                .collect(),
            ValueType::napi_boolean => members
                .iter()
                .filter(|member| {
                    matches!(member, Type::Bool | Type::LiteralBool(_) | Type::Union(_))
                })
                .collect(),
            ValueType::napi_number => {
                let whole = js::get_f64(env, value).fract() == 0.0;
                let mut ordered: Vec<&Type> = Vec::new();
                if whole {
                    ordered.extend(members.iter().filter(|m| matches!(m, Type::Int)));
                }
                ordered.extend(members.iter().filter(|m| matches!(m, Type::Float)));
                ordered.extend(
                    members
                        .iter()
                        .filter(|m| matches!(m, Type::LiteralInt(_) | Type::Union(_))),
                );
                ordered
            }
            ValueType::napi_string => {
                let mut ordered: Vec<&Type> = Vec::new();
                ordered.extend(
                    members
                        .iter()
                        .filter(|m| matches!(m, Type::LiteralString(_))),
                );
                ordered.extend(
                    members
                        .iter()
                        .filter(|m| matches!(m, Type::Enum(_) | Type::EnumVariant { .. })),
                );
                ordered.extend(
                    members
                        .iter()
                        .filter(|m| matches!(m, Type::String | Type::Union(_))),
                );
                ordered
            }
            ValueType::napi_object if js::is_array(env, value) => members
                .iter()
                .filter(|m| matches!(m, Type::List(_) | Type::Union(_)))
                .collect(),
            ValueType::napi_object => {
                // An instance of a generated class picks its class first.
                let constructor = js::get_named(env, value, c"constructor").ok();
                let mut ordered: Vec<&Type> = Vec::new();
                if let Some(constructor) = constructor {
                    ordered.extend(members.iter().filter(|member| match member {
                        Type::Class(class) => self.is_constructor_of(env, *class, constructor),
                        _ => false,
                    }));
                }
                ordered.extend(
                    members
                        .iter()
                        .filter(|m| matches!(m, Type::Class(_) | Type::Map(_) | Type::Union(_))),
                );
                ordered
            }
            _ => Vec::new(),
        };
        let mut last = None;
        for member in candidates {
            let base = path.len();
            match self.arg(env, session, member, value, path, buffer) {
                Ok(built) => return Ok(built),
                Err(ArgError::Js(thrown)) => return Err(ArgError::Js(thrown)),
                Err(unfit) => {
                    path.truncate(base);
                    last = Some(unfit);
                }
            }
        }
        // One candidate: its own reason is the clearest.
        match last {
            Some(unfit) if members.len() == 1 => Err(unfit),
            _ => Err(ArgError::Unfit(format!(
                "{path}: expected one of {}, found {}",
                self.render(ty),
                found(env, value)
            ))),
        }
    }

    fn is_constructor_of(&self, env: Env, class: u32, constructor: Value) -> bool {
        match self.types.get(class as usize) {
            Some(TypePlan::Class {
                constructor: Some(ours),
                ..
            }) => js::strict_equals(env, js::deref(env, *ours), constructor),
            _ => false,
        }
    }
}

fn refused(path: &str, error: &SdkError) -> ArgError {
    ArgError::Unfit(format!("{path}: {error}"))
}

/// An `int` as JavaScript has it: a number, as `Number(int)` in today's SDK.
fn int_value(env: Env, value: &BamlValue<'_>) -> Result<Value, CopyError> {
    #[expect(clippy::cast_precision_loss, reason = "what today's SDK does")]
    Ok(js::number(env, value.int()? as f64))
}

/// What a JavaScript value is, for an error message.
fn found(env: Env, value: Value) -> &'static str {
    match js::type_of(env, value) {
        ValueType::napi_undefined => "undefined",
        ValueType::napi_null => "null",
        ValueType::napi_boolean => "a boolean",
        ValueType::napi_number => {
            if js::get_f64(env, value).fract() == 0.0 {
                "a whole number"
            } else {
                "a number"
            }
        }
        ValueType::napi_string => "a string",
        ValueType::napi_object if js::is_array(env, value) => "an array",
        ValueType::napi_object => "an object",
        ValueType::napi_function => "a function",
        ValueType::napi_bigint => "a bigint",
        _ => "a value",
    }
}
