//! Python values encoded against the types the program declares.
//!
//! This is Python's answer to the walker in `bridge_encode`: how a Python
//! object relates to each kind of slot. The rules are Python's, decided here
//! and nowhere else:
//!
//! - `bool` is not an `int`; an `int` is an `int`, and fits a `float` slot
//!   when the float is exactly that integer, or a `bigint` slot always.
//! - A `float` fits only a `float` slot.
//! - An `Enum` member is a variant of its own enum; a `str` names a variant
//!   when the slot is an enum.
//! - A pydantic instance is an instance of its class; a `dict` with string
//!   keys stands in for a class when every key is a declared field.
//! - A callable becomes a host callable bound to a function slot, and the
//!   slot's signature is recorded so the result can be encoded against it.
//! - Handles, media, prompts and type values are host-owned leaves.
//!
//! The declared types come from the generated SDK's program descriptor,
//! registered once by `set_type_map`. A call to a function the descriptor
//! does not know (call-by-name without a generated SDK) encodes each
//! argument as the type its Python value has.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, RwLock},
};

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{MediaKind, Name, RuntimeTy, TypeName};
use bridge_cffi::handle_cffi;
use bridge_ctypes::{
    HANDLE_TABLE,
    baml_bridge::cffi::{
        BamlHandle, BamlHandleType, BamlTy, BamlTyDef, BamlValueMedia, BamlValuePromptAst,
        CallFunctionArgs, InboundValue, call_function_args::CallTarget, inbound_value::Value,
    },
};
use bridge_encode::{
    Answer, EncodeError, Facts, FunctionSig, HostError, HostValue, Instance, Leaf, LeafKind, Param,
    PathSegment, Scalar, ScalarSlot, encode, encode_arguments,
};
use num_bigint::BigInt;
use prost::Message;
use pyo3::{
    Bound, PyAny, PyErr, PyResult, Python,
    exceptions::PyTypeError,
    prelude::*,
    types::{
        PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple,
        PyType,
    },
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::{media, py_handle::BamlPyHandle};

/// The declared types of the loaded program, from the generated SDK. `None`
/// until a typemap with a descriptor is installed.
static FACTS: RwLock<Option<Arc<Facts>>> = RwLock::new(None);

/// Install the program descriptor a generated SDK embeds in its typemap
/// (`set_type_map` calls this), or clear it with `None`.
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (descriptor))]
pub fn register_program_descriptor(descriptor: Option<Vec<u8>>) -> PyResult<()> {
    let facts = match descriptor {
        Some(bytes) => Some(Arc::new(Facts::from_descriptor(
            &ProgramDescriptor::decode_embedded(&bytes)
                .map_err(|error| PyTypeError::new_err(error.to_string()))?,
        ))),
        None => None,
    };
    *FACTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = facts;
    Ok(())
}

fn facts() -> Arc<Facts> {
    FACTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

/// Encode a call's keyword arguments (and cancel token) into `call`, against
/// the target's declared parameter types.
///
/// `call` already carries the control fields Python built: identity, target,
/// explicit type arguments, invocation options. Every owner this creates (a
/// registered callable, a cloned handle) is released again if encoding
/// fails, because the engine will never see the bytes.
pub(crate) fn encode_call_kwargs(
    py: Python<'_>,
    call: &mut CallFunctionArgs,
    kwargs: Option<&Bound<'_, PyDict>>,
    cancel: Option<&Bound<'_, PyAny>>,
) -> PyResult<()> {
    let facts = facts();
    let cx = Cx::new(py)?;
    let outcome = (|| -> Result<(), Failure> {
        if let Some(cancel) = cancel {
            let encoded = encode(
                &facts,
                &cx.value(cancel.clone()),
                &RuntimeTy::Unknown,
                vec![PathSegment::Argument("_baml.cancel".into())],
            )?;
            call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded.value);
        }
        let Some(kwargs) = kwargs else {
            return Ok(());
        };
        let arguments = kwargs
            .iter()
            .map(|(name, value)| Ok((name.extract::<String>()?, cx.value(value))))
            .collect::<PyResult<Vec<_>>>()?;
        let (target, signature) = target_signature(&facts, call.call_target.as_ref())?;
        let bindings = bridge_ctypes::proto_ty_args_to_named(&call.type_args)
            .map_err(|error| PyTypeError::new_err(error.to_string()))?
            .type_args
            .into_iter()
            .map(|(name, ty)| (Name::new(&name), ty))
            .collect::<HashMap<_, _>>();
        call.kwargs = match &signature {
            Some(signature) => encode_arguments(&facts, signature, &bindings, &arguments)
                .map_err(|error| Failure::Encode(target.clone(), error))?,
            // No declared types: each argument is the type its value has.
            None => arguments
                .iter()
                .map(|(name, value)| {
                    let encoded = encode(
                        &facts,
                        value,
                        &RuntimeTy::Unknown,
                        vec![PathSegment::Argument(name.clone())],
                    )
                    .map_err(|error| Failure::Encode(target.clone(), error))?;
                    Ok(bridge_ctypes::baml_bridge::cffi::InboundMapEntry {
                        key: Some(
                            bridge_ctypes::baml_bridge::cffi::inbound_map_entry::Key::StringKey(
                                name.clone(),
                            ),
                        ),
                        value: Some(encoded.value),
                    })
                })
                .collect::<Result<Vec<_>, Failure>>()?,
        };
        Ok(())
    })();
    cx.finish(outcome)
}

/// Encode one Python value as the bridge would send it where the program
/// declares no type: the value's own type, stated on the wire where a
/// container or class needs it. Returns `InboundValue` bytes; raises
/// `TypeError` for a value the bridge cannot send. For inspection and tests.
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (value, name = "value"))]
pub fn _encode_inbound<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    encode_value_for(
        py,
        value,
        &RuntimeTy::Unknown,
        PathSegment::Argument(name.to_string()),
        "",
    )
    .map(|value| PyBytes::new(py, &value.encode_to_vec()))
}

/// Encode a host callable's result or thrown value against `slot`.
pub(crate) fn encode_value(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    slot: &RuntimeTy,
    root: PathSegment,
) -> PyResult<InboundValue> {
    encode_value_for(py, value, slot, root, "host callable")
}

/// Encode one Python value against `slot` for a path that starts at `root`;
/// `target` names the call in a `TypeError`, or is empty for none.
fn encode_value_for(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    slot: &RuntimeTy,
    root: PathSegment,
    target: &str,
) -> PyResult<InboundValue> {
    let facts = facts();
    let cx = Cx::new(py)?;
    let outcome = encode(&facts, &cx.value(value.clone()), slot, vec![root])
        .map(|encoded| encoded.value)
        .map_err(|error| Failure::Encode(target.to_string(), error));
    cx.finish(outcome)
}

/// The signature a call target declares: from the descriptor for a named
/// function, from the live handle for a BAML callable, or none.
fn target_signature(
    facts: &Facts,
    target: Option<&CallTarget>,
) -> Result<(String, Option<FunctionSig>), Failure> {
    match target {
        Some(CallTarget::FunctionName(name)) => Ok((name.clone(), facts.function(name).cloned())),
        Some(CallTarget::FunctionHandle(key)) => {
            let entry = HANDLE_TABLE.resolve(*key);
            let signature = match entry.as_deref() {
                Some(bridge_ctypes::CffiHandleTableEntry::Adt(
                    bex_project::BexExternalAdt::TaggedHeapHandle {
                        ty:
                            RuntimeTy::Function {
                                params,
                                ret,
                                throws,
                            },
                        ..
                    },
                )) => Some(FunctionSig {
                    class_generic_params: Vec::new(),
                    generic_params: Vec::new(),
                    params: params
                        .iter()
                        .enumerate()
                        .map(|(index, param)| Param {
                            name: param
                                .name
                                .as_ref()
                                .map_or_else(|| format!("arg{index}"), ToString::to_string),
                            ty: param.ty.clone(),
                            has_default: param.is_optional(),
                        })
                        .collect(),
                    ret: (**ret).clone(),
                    throws: (**throws).clone(),
                }),
                // Not a callable, or no longer live: the request decoder
                // reports that; the arguments go as their own types.
                Some(_) | None => None,
            };
            Ok(("callable".into(), signature))
        }
        None => Ok(("call".into(), None)),
    }
}

/// Why an encode stopped. A Python error raised while reading a value is
/// kept as it is; a walker error becomes a `TypeError`.
enum Failure {
    Python(PyErr),
    Encode(String, EncodeError),
}

impl From<PyErr> for Failure {
    fn from(error: PyErr) -> Self {
        Failure::Python(error)
    }
}

impl From<EncodeError> for Failure {
    fn from(error: EncodeError) -> Self {
        Failure::Encode("host callable".into(), error)
    }
}

/// What one encode shares across every value it visits: the Python helpers
/// it needs, and the owners it created and may have to release.
pub(crate) struct Cx<'py> {
    py: Python<'py>,
    type_map: Bound<'py, PyAny>,
    proto: Bound<'py, PyModule>,
    enum_class: Bound<'py, PyAny>,
    base_model: Option<Bound<'py, PyAny>>,
    class_instance: Bound<'py, PyAny>,
    /// `baml_bridge.proto._PortablePromptAst`, the owned wire copy of a prompt.
    prompt_ast: Bound<'py, PyAny>,
    /// `baml_bridge.proto.BamlType`, a reflected type definition.
    baml_type: Bound<'py, PyAny>,
    /// `baml_bridge.proto._BamlTypeMetadataRow`, a record of five attributes.
    metadata_row: Bound<'py, PyAny>,
    /// Streams, function specs, runtime values and BAML closures: Python
    /// wrappers around an engine handle, reached through `_to_pyhandle`.
    handle_wrappers: [Bound<'py, PyAny>; 4],
    /// Host callables registered during this encode.
    registered: RefCell<Vec<u64>>,
    /// Handle keys cloned for the wire during this encode.
    cloned: RefCell<Vec<u64>>,
    /// The first Python error raised while answering the walker. The walker
    /// sees a non-fit; the caller raises this instead of the walker's error.
    failure: RefCell<Option<PyErr>>,
}

impl<'py> Cx<'py> {
    fn new(py: Python<'py>) -> PyResult<Self> {
        let proto = PyModule::import(py, "baml_bridge.proto")?;
        let typemap = PyModule::import(py, "baml_bridge.typemap")?;
        Ok(Self {
            py,
            type_map: typemap.getattr("get_type_map")?.call0()?,
            enum_class: PyModule::import(py, "enum")?.getattr("Enum")?,
            base_model: PyModule::import(py, "pydantic")
                .and_then(|pydantic| pydantic.getattr("BaseModel"))
                .ok(),
            class_instance: typemap.getattr("BamlClassInstance")?,
            prompt_ast: proto.getattr("_PortablePromptAst")?,
            baml_type: proto.getattr("BamlType")?,
            metadata_row: proto.getattr("_BamlTypeMetadataRow")?,
            handle_wrappers: [
                proto.getattr("BamlStream")?,
                proto.getattr("BamlFunctionSpec")?,
                proto.getattr("BamlRuntimeValue")?,
                proto.getattr("BamlClosure")?,
            ],
            proto,
            registered: RefCell::new(Vec::new()),
            cloned: RefCell::new(Vec::new()),
            failure: RefCell::new(None),
        })
    }

    fn value<'a>(&'a self, obj: Bound<'py, PyAny>) -> PyValue<'a, 'py> {
        PyValue { obj, cx: self }
    }

    /// Release what this encode created when it did not reach the engine,
    /// and turn the outcome into Python's.
    fn finish<T>(self, outcome: Result<T, Failure>) -> PyResult<T> {
        match outcome {
            Ok(value) => Ok(value),
            Err(failure) => {
                for key in self.registered.borrow().iter() {
                    crate::host_value::release_registered(*key);
                }
                for key in self.cloned.borrow().iter() {
                    // Best effort: a key the engine never saw has no other owner.
                    let _ = handle_cffi::release_handle(*key);
                }
                if let Some(error) = self.failure.borrow_mut().take() {
                    return Err(error);
                }
                Err(match failure {
                    Failure::Python(error) => error,
                    Failure::Encode(target, error) if target.is_empty() => {
                        PyTypeError::new_err(error.to_string())
                    }
                    Failure::Encode(target, error) => {
                        PyTypeError::new_err(format!("{target}: {error}"))
                    }
                })
            }
        }
    }

    /// Record a Python error raised while answering; the walker is told the
    /// value does not fit, and the error is what the caller gets.
    fn fail<T>(&self, error: PyErr) -> Option<T> {
        self.failure.borrow_mut().get_or_insert(error);
        None
    }

    fn is_enum(&self, obj: &Bound<'py, PyAny>) -> bool {
        obj.is_instance(&self.enum_class).unwrap_or(false)
    }

    fn is_model(&self, obj: &Bound<'py, PyAny>) -> bool {
        self.base_model
            .as_ref()
            .is_some_and(|base| obj.is_instance(base).unwrap_or(false))
    }

    /// The BAML name the typemap registers for `class`, if any.
    fn baml_name(&self, class: &Bound<'py, PyAny>) -> PyResult<Option<TypeName>> {
        let fqn: String = self
            .type_map
            .call_method1("py_type_to_baml_type", (generic_origin(class)?,))?
            .extract()?;
        Ok((!fqn.is_empty()).then(|| TypeName::from_dotted_path(&fqn)))
    }

    fn wire_ty(&self, ty: &Bound<'py, PyAny>) -> PyResult<RuntimeTy> {
        let encoded: Vec<u8> = ty.call_method0("SerializeToString")?.extract()?;
        let ty = BamlTy::decode(encoded.as_slice())
            .map_err(|error| PyTypeError::new_err(error.to_string()))?;
        bridge_ctypes::proto_ty_to_runtime_ty(&ty)
            .map_err(|error| PyTypeError::new_err(error.to_string()))
    }
}

/// `Box[int]` is a runtime subclass pydantic makes; its BAML class is `Box`.
fn generic_origin<'py>(class: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if let Ok(meta) = class.getattr("__pydantic_generic_metadata__")
        && let Ok(origin) = meta.get_item("origin")
        && !origin.is_none()
    {
        return Ok(origin);
    }
    Ok(class.clone())
}

pub(crate) struct PyValue<'a, 'py> {
    obj: Bound<'py, PyAny>,
    cx: &'a Cx<'py>,
}

impl<'py> PyValue<'_, 'py> {
    fn is_pyclass<T: pyo3::PyTypeInfo>(&self) -> bool {
        self.obj.is_instance_of::<T>()
    }

    fn is_a(&self, class: &Bound<'py, PyAny>) -> bool {
        self.obj.is_instance(class).unwrap_or(false)
    }

    /// A `BamlPyHandle` cloned for the wire, with the clone tracked for
    /// rollback.
    fn handle_leaf(&self, handle: &BamlPyHandle) -> PyResult<Leaf> {
        let key = crate::py_handle::handle_clone(handle.handle_key, "inbound encode")?;
        self.cx.cloned.borrow_mut().push(key);
        Ok(Leaf {
            value: InboundValue {
                value_type: None,
                value: Some(Value::Handle(BamlHandle {
                    key,
                    handle_type: i32::try_from(handle.handle_type)
                        .map_err(|_| PyTypeError::new_err("handle type does not fit the wire"))?,
                })),
            },
            kind: LeafKind::Handle,
        })
    }

    fn prompt_leaf(&self, prompt: &Bound<'py, PyAny>) -> PyResult<Leaf> {
        let encoded: Vec<u8> = prompt
            .call_method0("wire_copy")?
            .call_method0("SerializeToString")?
            .extract()?;
        let ast = BamlValuePromptAst::decode(encoded.as_slice())
            .map_err(|error| PyTypeError::new_err(error.to_string()))?;
        Ok(Leaf {
            value: InboundValue {
                value_type: None,
                value: Some(Value::PromptAstValue(ast)),
            },
            kind: LeafKind::Prompt,
        })
    }

    fn media_leaf(&self, kind: MediaKind, payload: PyResult<BamlValueMedia>) -> PyResult<Leaf> {
        Ok(Leaf {
            value: InboundValue {
                value_type: None,
                value: Some(Value::MediaValue(payload?)),
            },
            kind: LeafKind::Media(kind),
        })
    }

    /// The leaf this object is, if it is one. `Ok(None)` for anything else.
    fn try_leaf(&self) -> PyResult<Option<Leaf>> {
        let py = self.cx.py;
        if let Ok(handle) = self.obj.cast::<BamlPyHandle>() {
            return self.handle_leaf(&handle.borrow()).map(Some);
        }
        if let Ok(image) = self.obj.cast::<media::BamlImage>() {
            return self
                .media_leaf(MediaKind::Image, image.borrow().wire_payload(py))
                .map(Some);
        }
        if let Ok(audio) = self.obj.cast::<media::BamlAudio>() {
            return self
                .media_leaf(MediaKind::Audio, audio.borrow().wire_payload(py))
                .map(Some);
        }
        if let Ok(video) = self.obj.cast::<media::BamlVideo>() {
            return self
                .media_leaf(MediaKind::Video, video.borrow().wire_payload(py))
                .map(Some);
        }
        if let Ok(pdf) = self.obj.cast::<media::BamlPdf>() {
            return self
                .media_leaf(MediaKind::Pdf, pdf.borrow().wire_payload(py))
                .map(Some);
        }
        if self.is_a(&self.cx.prompt_ast) {
            return self.prompt_leaf(&self.obj).map(Some);
        }
        if self.is_a(&self.cx.baml_type) {
            let encoded: Vec<u8> = self
                .obj
                .call_method0("_wire_copy")?
                .call_method0("SerializeToString")?
                .extract()?;
            let definition = BamlTyDef::decode(encoded.as_slice())
                .map_err(|error| PyTypeError::new_err(error.to_string()))?;
            return Ok(Some(Leaf {
                value: InboundValue {
                    value_type: None,
                    value: Some(Value::TyDefValue(definition)),
                },
                kind: LeafKind::TypeValue,
            }));
        }
        // Streams, function specs, runtime values and BAML closures wrap an
        // engine handle.
        if self.cx.handle_wrappers.iter().any(|class| self.is_a(class)) {
            let handle = self.obj.call_method0("_to_pyhandle")?;
            let handle = handle.cast::<BamlPyHandle>()?;
            // `_to_pyhandle` already cloned the key for us; `handle_leaf`
            // clones once more, so release the intermediate clone with
            // the Python object as usual.
            return self.handle_leaf(&handle.borrow()).map(Some);
        }
        // The generated `ai.Prompt` shell carries its AST in `_data`.
        if self.cx.is_model(&self.obj) {
            let private = self.obj.getattr("__pydantic_private__")?;
            if !private.is_none()
                && let Ok(data) = private.get_item("_data")
                && data.is_instance(&self.cx.prompt_ast)?
            {
                return self.prompt_leaf(&data).map(Some);
            }
            for (wire_name, value) in self.model_fields()? {
                if wire_name == "_data" && value.is_instance(&self.cx.prompt_ast)? {
                    return self.prompt_leaf(&value).map(Some);
                }
            }
        }
        Ok(None)
    }

    /// A pydantic instance's fields under their wire names, declared fields
    /// first, then private attributes that carry engine values.
    fn model_fields(&self) -> PyResult<Vec<(String, Bound<'py, PyAny>)>> {
        let class = self.obj.get_type();
        let infos = class.getattr("model_fields")?;
        let infos = infos.cast::<PyDict>()?;
        let mut fields = Vec::new();
        for item in self.obj.try_iter()? {
            let (name, value): (String, Bound<'py, PyAny>) = item?.extract()?;
            let wire_name = match infos.get_item(&name)? {
                Some(info) => {
                    let serialization = info.getattr("serialization_alias")?;
                    let alias = info.getattr("alias")?;
                    if !serialization.is_none() {
                        serialization.extract()?
                    } else if !alias.is_none() {
                        alias.extract()?
                    } else {
                        name
                    }
                }
                None => name,
            };
            fields.push((wire_name, value));
        }
        let private = self.obj.getattr("__pydantic_private__")?;
        if !private.is_none() {
            for (name, value) in private.cast::<PyDict>()?.iter() {
                let keep = value.is_instance_of::<BamlPyHandle>()
                    || value.is_instance(&self.cx.prompt_ast)?;
                if keep {
                    fields.push((name.extract()?, value));
                }
            }
        }
        Ok(fields)
    }

    fn try_instance(&self) -> PyResult<Option<Instance<Self>>> {
        if self.cx.is_model(&self.obj) {
            let Some(class) = self.cx.baml_name(&self.obj.get_type().into_any())? else {
                return Ok(None);
            };
            let args: Vec<Bound<'py, PyAny>> = self
                .cx
                .proto
                .getattr("pydantic_instance_type_args")?
                .call1((&self.obj,))?
                .extract()?;
            let type_args = if args.is_empty() {
                None
            } else {
                Some(
                    args.iter()
                        .map(|arg| {
                            if arg.is_instance(&self.cx.baml_type)? {
                                let definition = arg.call_method0("_wire_copy")?;
                                self.cx.wire_ty(&definition.getattr("root")?)
                            } else {
                                let wire = self
                                    .cx
                                    .proto
                                    .getattr("python_type_to_wire_ty")?
                                    .call1((arg,))?;
                                self.cx.wire_ty(&wire)
                            }
                        })
                        .collect::<PyResult<Vec<_>>>()?,
                )
            };
            let fields = self
                .model_fields()?
                .into_iter()
                .map(|(name, value)| (name, self.cx.value(value)))
                .collect();
            return Ok(Some(Instance {
                class,
                type_args,
                fields,
            }));
        }
        if self.obj.is_instance(&self.cx.class_instance)? {
            let class: String = self.obj.getattr("class_name")?.extract()?;
            let fields = self.obj.getattr("fields")?;
            let fields = fields
                .cast::<PyDict>()?
                .iter()
                .map(|(name, value)| Ok((name.extract::<String>()?, self.cx.value(value))))
                .collect::<PyResult<Vec<_>>>()?;
            return Ok(Some(Instance {
                class: TypeName::from_dotted_path(&class),
                type_args: None,
                fields,
            }));
        }
        Ok(None)
    }

    fn try_map(&self) -> PyResult<Option<Vec<(Self, Self)>>> {
        if let Ok(dict) = self.obj.cast::<PyDict>() {
            return Ok(Some(
                dict.iter()
                    .map(|(key, value)| (self.cx.value(key), self.cx.value(value)))
                    .collect(),
            ));
        }
        // A type metadata row is a plain record of five attributes.
        if self.is_a(&self.cx.metadata_row) {
            let py = self.cx.py;
            return ["ty", "alias", "description", "docstring", "other"]
                .iter()
                .map(|name| {
                    Ok((
                        self.cx.value(PyString::new(py, name).into_any()),
                        self.cx.value(self.obj.getattr(*name)?),
                    ))
                })
                .collect::<PyResult<Vec<_>>>()
                .map(Some);
        }
        Ok(None)
    }

    fn try_enum_member(&self) -> PyResult<Option<(TypeName, String)>> {
        if !self.cx.is_enum(&self.obj) {
            return Ok(None);
        }
        let Some(name) = self.cx.baml_name(&self.obj.get_type().into_any())? else {
            return Ok(None);
        };
        let value = self.obj.getattr("value")?;
        let Ok(variant) = value.extract::<String>() else {
            return Err(PyTypeError::new_err(format!(
                "cannot encode enum member {}: BAML enum values are strings",
                self.describe()
            )));
        };
        Ok(Some((name, variant)))
    }

    /// Python's `int` as the slot asks: the same number as an `int` when it
    /// fits, a `bigint` by conversion, a `float` by conversion only when
    /// exact, and under an open slot whatever width it needs.
    fn int_scalar(&self, slot: ScalarSlot) -> PyResult<Answer<Scalar>> {
        let small = self.obj.extract::<i64>().ok();
        Ok(match (slot, small) {
            (ScalarSlot::Int, Some(int)) => Answer::AsIs(Scalar::Int(int)),
            (ScalarSlot::Int, None) => Answer::No,
            (ScalarSlot::Bigint, Some(int)) => Answer::Converted(Scalar::Bigint(BigInt::from(int))),
            (ScalarSlot::Bigint, None) => Answer::Converted(Scalar::Bigint(self.big_int()?)),
            (ScalarSlot::Float, Some(int)) => match exact_float(int) {
                Some(float) => Answer::Converted(Scalar::Float(float)),
                None => Answer::No,
            },
            (ScalarSlot::Float, None) => Answer::No,
            (ScalarSlot::Open, Some(int)) => Answer::AsIs(Scalar::Int(int)),
            (ScalarSlot::Open, None) => Answer::AsIs(Scalar::Bigint(self.big_int()?)),
            (ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes, _) => Answer::No,
        })
    }

    fn big_int(&self) -> PyResult<BigInt> {
        let hex: String = self
            .cx
            .py
            .import("builtins")?
            .getattr("format")?
            .call1((&self.obj, "x"))?
            .extract()?;
        BigInt::parse_bytes(hex.as_bytes(), 16)
            .ok_or_else(|| PyTypeError::new_err("Python int did not format as base sixteen"))
    }
}

/// A scalar that is one BAML scalar as it is: it fits the slot of that kind,
/// or an open slot, and nothing else.
fn only(slot: ScalarSlot, kind: ScalarSlot, scalar: Scalar) -> Answer<Scalar> {
    if slot == kind || slot == ScalarSlot::Open {
        Answer::AsIs(scalar)
    } else {
        Answer::No
    }
}

/// Python's rule for an `int` in a `float` slot: only when the float is
/// exactly that integer.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the round trip checks the cast was exact"
)]
fn exact_float(int: i64) -> Option<f64> {
    let float = int as f64;
    (float as i64 == int).then_some(float)
}

impl<'py> HostValue for PyValue<'_, 'py> {
    fn is_null(&self) -> bool {
        self.obj.is_none()
    }

    fn scalar(&self, slot: ScalarSlot) -> Answer<Scalar> {
        if self.cx.is_enum(&self.obj) {
            // An enum member is a variant, whatever it mixes in.
            return Answer::No;
        }
        // Every Python scalar but `int` is exactly one BAML scalar and fits
        // only that slot (or an open one); `int` has rules of its own.
        if let Ok(bool) = self.obj.cast::<PyBool>() {
            return only(slot, ScalarSlot::Bool, Scalar::Bool(bool.is_true()));
        }
        if self.is_pyclass::<PyInt>() {
            return match self.int_scalar(slot) {
                Ok(answer) => answer,
                Err(error) => self.cx.fail(error).unwrap_or(Answer::No),
            };
        }
        if let Ok(float) = self.obj.cast::<PyFloat>() {
            return only(slot, ScalarSlot::Float, Scalar::Float(float.value()));
        }
        if let Ok(string) = self.obj.cast::<PyString>() {
            return match string.to_str() {
                Ok(string) => only(slot, ScalarSlot::String, Scalar::String(string.to_owned())),
                Err(error) => self.cx.fail(error).unwrap_or(Answer::No),
            };
        }
        if let Ok(bytes) = self.obj.cast::<PyBytes>() {
            return only(
                slot,
                ScalarSlot::Bytes,
                Scalar::Bytes(bytes.as_bytes().to_vec()),
            );
        }
        if let Ok(bytes) = self.obj.cast::<PyByteArray>() {
            return only(slot, ScalarSlot::Bytes, Scalar::Bytes(bytes.to_vec()));
        }
        Answer::No
    }

    fn enum_member(&self) -> Option<(TypeName, String)> {
        match self.try_enum_member() {
            Ok(member) => member,
            Err(error) => self.cx.fail(error),
        }
    }

    fn enum_variant(&self, enumeration: &bridge_encode::EnumFacts) -> Answer<String> {
        if !self.cx.is_enum(&self.obj)
            && let Ok(string) = self.obj.cast::<PyString>()
        {
            // A string names a variant; the walker checks it is one.
            return match string.to_str() {
                Ok(string) => Answer::Converted(string.to_owned()),
                Err(error) => self.cx.fail(error).unwrap_or(Answer::No),
            };
        }
        match self.enum_member() {
            Some((name, variant)) if name == enumeration.name => Answer::AsIs(variant),
            Some(_) | None => Answer::No,
        }
    }

    fn list(&self) -> Option<Vec<Self>> {
        if !(self.is_pyclass::<PyList>() || self.is_pyclass::<PyTuple>()) {
            return None;
        }
        // Through the protocol, not the C-level items, so a subclass that
        // overrides `__iter__` is read the way Python reads it.
        let items = self
            .obj
            .try_iter()
            .and_then(|iter| iter.collect::<PyResult<Vec<_>>>());
        match items {
            Ok(items) => Some(items.into_iter().map(|item| self.cx.value(item)).collect()),
            Err(error) => self.cx.fail(error),
        }
    }

    fn map(&self) -> Option<Vec<(Self, Self)>> {
        match self.try_map() {
            Ok(entries) => entries,
            Err(error) => self.cx.fail(error),
        }
    }

    fn instance(&self) -> Option<Instance<Self>> {
        match self.try_instance() {
            Ok(instance) => instance,
            Err(error) => self.cx.fail(error),
        }
    }

    fn is_callable(&self) -> bool {
        // Classes are callable but are types, not functions; pydantic
        // instances may be callable too and are values of their class.
        self.obj.is_callable() && !self.is_pyclass::<PyType>() && !self.cx.is_model(&self.obj)
    }

    fn callable(&self, signature: &RuntimeTy) -> Option<Result<InboundValue, HostError>> {
        if !self.is_callable() {
            return None;
        }
        let registration = (|| -> PyResult<u64> {
            let marker = PyModule::import(self.cx.py, "baml_bridge._host_marker")?
                .getattr("marker_for")?
                .call1((&self.obj,))?;
            let marker = if marker.is_none() {
                None
            } else {
                Some(marker.cast::<BamlPyHandle>()?.borrow().handle_key)
            };
            crate::host_value::register_callable(
                self.obj.clone().unbind(),
                marker,
                signature.clone(),
            )
        })();
        Some(match registration {
            Ok(key) => {
                self.cx.registered.borrow_mut().push(key);
                Ok(InboundValue {
                    value_type: None,
                    value: Some(Value::Handle(BamlHandle {
                        key,
                        handle_type: BamlHandleType::HostValueCallable as i32,
                    })),
                })
            }
            Err(error) => {
                let message = error.to_string();
                self.cx.fail::<()>(error);
                Err(HostError(message))
            }
        })
    }

    fn leaf(&self) -> Option<Result<Leaf, HostError>> {
        match self.try_leaf() {
            Ok(leaf) => leaf.map(Ok),
            Err(error) => {
                let message = error.to_string();
                self.cx.fail::<()>(error);
                Some(Err(HostError(message)))
            }
        }
    }

    fn describe(&self) -> String {
        const LIMIT: usize = 48;
        let type_name = self
            .obj
            .get_type()
            .name()
            .map(|name| name.to_string())
            .unwrap_or_else(|_| "object".to_string());
        let repr = self
            .obj
            .repr()
            .and_then(|repr| repr.to_str().map(str::to_owned))
            .unwrap_or_default();
        let mut shown: String = repr.chars().take(LIMIT).collect();
        if shown.len() < repr.len() {
            shown.push('…');
        }
        format!("{type_name} {shown}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_float_round_trips_only_representable_ints() {
        assert_eq!(exact_float(3), Some(3.0));
        assert_eq!(exact_float(-(1 << 53)), Some(-9_007_199_254_740_992.0));
        assert_eq!(exact_float((1 << 53) + 1), None);
    }
}
