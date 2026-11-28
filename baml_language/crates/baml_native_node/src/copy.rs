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

use crate::{
    args::{ArgError, MAX_DEPTH, Walk, is_inherited_name},
    js::{self, Env, JsResult, Ref, Thrown, Value},
};

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

pub(crate) struct FieldPlan {
    pub(crate) name: String,
    /// The field's name as a JavaScript string, kept alive.
    pub(crate) key: Ref,
    pub(crate) ty: Type,
    out: Out,
    pub(crate) nullable: bool,
    /// Object.prototype has a property of this name: the field is read only
    /// when it is the object's own.
    pub(crate) inherited_name: bool,
}

pub(crate) enum TypePlan {
    Class {
        name: String,
        fields: Vec<FieldPlan>,
        /// Each field's place, by name.
        field_index: HashMap<String, usize>,
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
    pub(crate) types: Vec<TypePlan>,
    /// `() => Object.create(null)`.
    make_map: Ref,
    /// The symbol `bind` sets on each generated class's prototype, to that
    /// class's number in the interface.
    pub(crate) class_tag: Ref,
    /// This realm's `Object.prototype`.
    pub(crate) object_prototype: Ref,
    /// Today's bridge's short rendering of a JavaScript value for an error
    /// message (`object with 2 keys`, `string "x"`, `Pair`).
    describe: Ref,
}

/// Why a copy out failed.
pub(crate) enum CopyError {
    /// JavaScript threw (a constructor): what it threw.
    Js(Value),
    /// The value did not have the shape its type says, or nests deeper than
    /// [`MAX_DEPTH`]: why.
    Decode(String),
}

/// A result that does not have the shape its type says.
fn decode(why: &str) -> CopyError {
    CopyError::Decode(format!("unexpected value from BAML: {why}"))
}

fn too_deep() -> CopyError {
    CopyError::Decode(format!(
        "the result is nested too deeply for the boundary, which carries {MAX_DEPTH} levels"
    ))
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

fn admits_null(ty: &Type) -> bool {
    match ty {
        Type::Null => true,
        Type::Union(members) => members.iter().any(admits_null),
        _ => false,
    }
}

/// The name of an interface type as the typemap and today's errors spell
/// it: user code lives under `user.`.
pub(crate) fn user_name(name: &str) -> String {
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
    #[expect(clippy::too_many_arguments, reason = "bind's arguments, one each")]
    pub(crate) fn build(
        env: Env,
        library: &Library,
        constructors: Value,
        factories: Value,
        enum_values: Value,
        make_map: Value,
        mode: u64,
        class_tag: Value,
        describe: Value,
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
                            inherited_name: is_inherited_name(&field.name),
                        })
                        .collect(),
                    field_index: fields
                        .iter()
                        .enumerate()
                        .map(|(i, field)| (field.name.clone(), i))
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
        let global = js::global(env);
        let object = js::get_named(env, global, c"Object")?;
        let object_prototype = js::get_named(env, object, c"prototype")?;
        Ok(Self {
            functions,
            types,
            make_map: js::reference(env, make_map),
            class_tag: js::reference(env, class_tag),
            object_prototype: js::reference(env, object_prototype),
            describe: js::reference(env, describe),
        })
    }

    /// Delete every reference the plans hold: they are being replaced.
    pub(crate) fn release(self, env: Env) {
        for plan in self.types {
            match plan {
                TypePlan::Class {
                    fields,
                    constructor,
                    factory,
                    ..
                } => {
                    for field in fields {
                        js::unreference(env, field.key);
                    }
                    for reference in [constructor, factory].into_iter().flatten() {
                        js::unreference(env, reference);
                    }
                }
                TypePlan::Enum { values, .. } => {
                    for value in values {
                        js::unreference(env, value);
                    }
                }
            }
        }
        for reference in [
            self.make_map,
            self.class_tag,
            self.object_prototype,
            self.describe,
        ] {
            js::unreference(env, reference);
        }
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
        self.copy(env, out, value, 0)
    }

    fn copy(
        &self,
        env: Env,
        out: &Out,
        value: &BamlValue<'_>,
        depth: usize,
    ) -> Result<Value, CopyError> {
        Ok(match out {
            Out::Int => int_value(env, value)?,
            Out::Float => js::number(env, value.float()?),
            Out::Bool => js::boolean(env, value.bool()?),
            Out::Null => js::null(env),
            Out::String => js::string(env, &value.string()?),
            Out::Class(class) => self.copy_class(env, *class, value, depth + 1)?,
            Out::Enum(of) => self.copy_enum(env, *of, value)?,
            Out::List(element_out) => {
                if depth >= MAX_DEPTH {
                    return Err(too_deep());
                }
                let elements = value.elements()?;
                let (length, _) = elements.size_hint();
                let array = js::array(env, length);
                for (i, element) in elements.enumerate() {
                    let element = self.copy(env, element_out, &element?, depth + 1)?;
                    js::set_element(env, array, js::index(i), element)?;
                }
                array
            }
            Out::Map(entry) => {
                if depth >= MAX_DEPTH {
                    return Err(too_deep());
                }
                let map = js::call(env, js::deref(env, self.make_map), &[])?;
                for (key, item) in value.entries()? {
                    let key = js::string(env, &key.string()?);
                    let item = self.copy(env, entry, &item, depth + 1)?;
                    js::set_property(env, map, key, item)?;
                }
                map
            }
            Out::Dynamic => self.copy_dynamic(env, value, depth)?,
        })
    }

    /// A value copied by its own tag.
    fn copy_dynamic(
        &self,
        env: Env,
        value: &BamlValue<'_>,
        depth: usize,
    ) -> Result<Value, CopyError> {
        let raw = value.raw();
        if raw.tag & tag::HANDLE == 0 {
            return Ok(match raw.tag {
                tag::NULL => js::null(env),
                tag::BOOL => js::boolean(env, value.bool()?),
                tag::INT => int_value(env, value)?,
                tag::FLOAT => js::number(env, value.float()?),
                other => return Err(decode(&format!("a scalar of tag {other}"))),
            });
        }
        Ok(match value.kind() {
            tag::STRING | tag::STRING_SLICE => js::string(env, &value.string()?),
            tag::INSTANCE => match value.class_of() {
                Some(class) => self.copy_class(env, class, value, depth + 1)?,
                // An instance of a class the interface does not describe
                // (the standard library's): its text.
                None => js::string(env, &value.describe()),
            },
            tag::ENUM_VARIANT => match value.class_of() {
                Some(of) => self.copy_enum(env, of, value)?,
                None => js::string(env, &value.describe()),
            },
            tag::ARRAY => self.copy(env, &Out::List(Box::new(Out::Dynamic)), value, depth)?,
            tag::MAP => self.copy(env, &Out::Map(Box::new(Out::Dynamic)), value, depth)?,
            other => return Err(decode(&format!("an object of tag {other}"))),
        })
    }

    fn copy_class(
        &self,
        env: Env,
        class: u32,
        value: &BamlValue<'_>,
        depth: usize,
    ) -> Result<Value, CopyError> {
        if depth > MAX_DEPTH {
            return Err(too_deep());
        }
        let Some(TypePlan::Class {
            fields,
            constructor,
            factory,
            ..
        }) = self.types.get(class as usize)
        else {
            return Err(decode(&format!("type {class} is not a class")));
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
            values[index] = self.copy(env, &field.out, &item, depth)?;
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
            return Err(decode(&format!("type {of} is not an enum")));
        };
        let index = value.variant_index()?;
        values
            .get(index as usize)
            .map(|variant| js::deref(env, *variant))
            .ok_or_else(|| decode(&format!("no variant {index} of enum {of}")))
    }

    /// What a call threw or panicked with, for JavaScript to build its error
    /// from: `{ value, className, text }`. `value` is the thrown value copied
    /// by its tag (a declared class arrives as its generated class);
    /// `className` its type's name when the interface has it, `text` the
    /// value as an uncaught throw of it is written.
    pub(crate) fn thrown(&self, env: Env, value: &BamlValue<'_>) -> Value {
        let object = js::object(env);
        let copied = match self.copy_dynamic(env, value, 0) {
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

    /// A refused argument or call, as today's bridge raises it: a plain
    /// `Error` with the code `InvalidArg`, its message led by the function.
    pub(crate) fn refusal(&self, env: Env, export: u32, why: &str) -> Value {
        let message = match self.functions.get(export as usize) {
            Some(function) => format!("{}: {why}", user_name(&function.name)),
            None => why.to_owned(),
        };
        js::error(env, Some("InvalidArg"), &message)
    }

    /// `value` as today's bridge renders it in an error message.
    pub(crate) fn describe(&self, env: Env, value: Value) -> String {
        let mut buffer = Vec::new();
        match js::call(env, js::deref(env, self.describe), &[value]) {
            Ok(text) if js::type_of(env, text) == ValueType::napi_string => {
                js::get_string(env, text, &mut buffer).to_owned()
            }
            _ => "value".to_owned(),
        }
    }

    /// The arguments of export `export`, built from the JavaScript array
    /// `args` (in parameter order; `undefined` for one not given) by
    /// [`Walk`]. Throws `invalid_argument` errors as today's SDK words them;
    /// what a getter throws is thrown as it is.
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
        let mut walk = Walk::new(self, env, session);
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
            match walk.argument(name, ty, arg) {
                Ok(value) => built.push(value),
                Err(ArgError::Js(thrown)) => return Err(thrown),
                Err(ArgError::Unfit(why) | ArgError::TooDeep(why)) => {
                    return Err(self.refusal(env, export, &why));
                }
            }
        }
        Ok(built)
    }

    pub(crate) fn render(&self, ty: &Type) -> String {
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
                    "{name}.{}",
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

    /// A type's name as today's bridge writes it after "expected": without
    /// the `user.` of its own code.
    pub(crate) fn type_name(&self, number: u32) -> String {
        match self.types.get(number as usize) {
            Some(TypePlan::Class { name, .. } | TypePlan::Enum { name, .. }) => name.clone(),
            None => format!("type {number}"),
        }
    }
}

/// An `int` as JavaScript has it: a number, as `Number(int)` in today's SDK.
fn int_value(env: Env, value: &BamlValue<'_>) -> Result<Value, CopyError> {
    #[expect(clippy::cast_precision_loss, reason = "what today's SDK does")]
    Ok(js::number(env, value.int()? as f64))
}
