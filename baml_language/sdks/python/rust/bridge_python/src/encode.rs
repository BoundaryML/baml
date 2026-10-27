//! Python values encoded against the types the program declares.
//!
//! This is Python's answer to the walker in `bridge_encode`: what a Python
//! object is, one level at a time ([`HostValue::shape`]), and what the bridge
//! does for one that ships ([`Host`]). The conversions are Python's
//! ([`Rules::Python`]), decided in the walker and nowhere else:
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
//! - Handles, media, prompts and type values are leaves the bridge
//!   serializes itself, once the walk has decided they ship.
//!
//! The declared types come from the generated SDK's program descriptor,
//! registered once by `set_type_map`. A call to a function the descriptor
//! does not know (call-by-name without a generated SDK) encodes each
//! argument as the type its Python value has.

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, RwLock},
};

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{Int63, MediaKind, Name, TypeName};
use bridge_cffi::handle_cffi;
use bridge_ctypes::{
    CffiHandleTableEntry, HANDLE_TABLE, HostKey, TableKey, WireHandle,
    baml_bridge::cffi::{
        BamlHandle, BamlTy, BamlValueMedia, BamlValuePromptAst, CallFunctionArgs,
        call_function_args::CallTarget,
    },
};
use bridge_encode::{
    Completion, Error, Facts, FunctionSig, Host, HostShape, HostValue, LeafKind, LeafWire, Owner,
    Owners, Param, Root, Rules, Scalar, SlotTy, StatedTy, encode, encode_arguments,
    state_type_args,
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
        Some(bytes) => {
            let descriptor = ProgramDescriptor::decode_embedded(&bytes)
                .map_err(|error| PyTypeError::new_err(error.to_string()))?;
            Some(Arc::new(
                Facts::from_descriptor(&descriptor)
                    .map_err(|error| PyTypeError::new_err(error.to_string()))?,
            ))
        }
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
    let mut owners = Owners::new(&cx);
    let (target, signature) = target_signature(&facts, call.call_target.as_ref());
    // The explicit type arguments as the wrappers hold them, restated with
    // references of the call's own: the wrappers keep theirs.
    let type_args: Vec<(String, StatedTy)> =
        bridge_ctypes::proto_ty_args_to_stated(&call.type_args, &HANDLE_TABLE)
            .map_err(|error| PyTypeError::new_err(error.to_string()))?
            .into_iter()
            .collect();
    call.type_args = state_type_args(&type_args, &mut owners)?;
    if let Some(cancel) = cancel {
        let encoded = encode(
            &facts,
            cx.value(cancel.clone()),
            &SlotTy::Unknown,
            Root::Argument("_baml.cancel".into()),
            &mut owners,
        )
        .map_err(|error| raise(&target, error))?;
        call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded);
    }
    if let Some(kwargs) = kwargs {
        let arguments = kwargs
            .iter()
            .map(|(name, value)| Ok((name.extract::<String>()?, cx.value(value))))
            .collect::<PyResult<Vec<_>>>()?;
        let bindings = type_args
            .iter()
            .map(|(name, ty)| (Name::new(name), ty.clone()))
            .collect::<HashMap<_, _>>();
        call.kwargs = encode_arguments(&facts, &signature, &bindings, arguments, &mut owners)
            .map_err(|error| raise(&target, error))?;
    }
    owners.keep();
    Ok(())
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
    let facts = facts();
    let cx = Cx::new(py)?;
    let mut owners = Owners::new(&cx);
    let encoded = encode(
        &facts,
        cx.value(value.clone()),
        &SlotTy::Unknown,
        Root::Argument(name.to_string()),
        &mut owners,
    )
    .map_err(|error| raise("", error))?;
    // The bytes carry what the encode created (a cloned handle's key), as
    // they would to the engine; the caller owns them and releases them.
    owners.keep();
    Ok(PyBytes::new(py, &encoded.encode_to_vec()))
}

/// Encode a host callable's result or thrown value against `slot`. What the
/// result means for the call is the caller's to decide from the
/// [`Completion`].
pub(crate) fn encode_result(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    slot: &StatedTy,
) -> Completion<PyErr> {
    let facts = facts();
    // The bridge's own setup failing (its Python module missing) is the
    // bridge's failure, never the callable raising.
    let cx = match Cx::new(py) {
        Ok(cx) => cx,
        Err(error) => return Completion::BridgeFailure(bridge_encode::BridgeFault::Host(error)),
    };
    let mut owners = Owners::new(&cx);
    let result = encode(
        &facts,
        cx.value(value.clone()),
        &SlotTy::from(slot),
        Root::Result,
        &mut owners,
    );
    if result.is_ok() {
        owners.keep();
    }
    Completion::from(result)
}

/// An encode's failure as Python raises it: a Python error as it is, a
/// refusal as a `TypeError` naming `target` (the call), or the refusal alone
/// when `target` is empty.
fn raise(target: &str, error: Error<PyErr>) -> PyErr {
    match error {
        Error::Host(error) | Error::Commit(error) => error,
        Error::Value(error) if target.is_empty() => PyTypeError::new_err(error.to_string()),
        Error::Value(error) => PyTypeError::new_err(format!("{target}: {error}")),
    }
}

/// The signature a call target declares: from the descriptor for a named
/// function, from the live handle for a BAML callable, or none, in which
/// case every argument goes as the type its value has.
fn target_signature(facts: &Facts, target: Option<&CallTarget>) -> (String, FunctionSig) {
    let untyped = || FunctionSig {
        class_generic_params: Vec::new(),
        generic_params: Vec::new(),
        params: Vec::new(),
        ret: SlotTy::Unknown,
        throws: SlotTy::Unknown,
    };
    match target {
        Some(CallTarget::FunctionName(name)) => (
            name.clone(),
            facts.function(name).cloned().unwrap_or_else(untyped),
        ),
        Some(CallTarget::FunctionHandle(key)) => {
            let entry = TableKey::new(*key).and_then(|key| HANDLE_TABLE.resolve(key));
            let signature = match entry.as_deref() {
                Some(bridge_ctypes::CffiHandleTableEntry::Adt(
                    bex_project::BexExternalAdt::TaggedHeapHandle {
                        ty:
                            bex_project::AnnotationTy::Function {
                                params,
                                ret,
                                throws,
                            },
                        ..
                    },
                )) => FunctionSig {
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
                            ty: param.ty.as_stated().into(),
                            has_default: param.is_optional(),
                        })
                        .collect(),
                    ret: ret.as_stated().into(),
                    throws: throws.as_stated().into(),
                },
                // Not a callable, or no longer live: the request decoder
                // reports that; the arguments go as their own types.
                Some(_) | None => untyped(),
            };
            ("callable".into(), signature)
        }
        None => ("call".into(), untyped()),
    }
}

/// What one encode shares across every value it visits: the Python helpers
/// it needs to read a value and to ship one.
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
    /// Streams, function specs, runtime values, opaque standard-library
    /// instances and BAML closures: Python wrappers around an engine handle,
    /// reached through `_to_pyhandle`.
    handle_wrappers: [Bound<'py, PyAny>; 5],
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
                proto.getattr("BamlOpaque")?,
                proto.getattr("BamlClosure")?,
            ],
            proto,
        })
    }

    fn value<'a>(&'a self, obj: Bound<'py, PyAny>) -> PyValue<'a, 'py> {
        PyValue { obj, cx: self }
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

    fn wire_ty(&self, ty: &Bound<'py, PyAny>) -> PyResult<StatedTy> {
        let encoded: Vec<u8> = ty.call_method0("SerializeToString")?.extract()?;
        let ty = BamlTy::decode(encoded.as_slice())
            .map_err(|error| PyTypeError::new_err(error.to_string()))?;
        bridge_ctypes::proto_ty_to_stated(&ty, &HANDLE_TABLE)
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

/// A protobuf message's bytes, through its `SerializeToString`.
fn message_bytes(message: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    message.call_method0("SerializeToString")?.extract()
}

impl<'py> Host<PyValue<'_, 'py>> for Cx<'py> {
    fn register(&self, callable: &PyValue<'_, 'py>, signature: &StatedTy) -> PyResult<HostKey> {
        let marker = PyModule::import(self.py, "baml_bridge._host_marker")?
            .getattr("marker_for")?
            .call1((&callable.obj,))?;
        let marker = if marker.is_none() {
            None
        } else {
            Some(marker.cast::<BamlPyHandle>()?.borrow().handle_key)
        };
        crate::host_value::register_callable(
            callable.obj.clone().unbind(),
            marker,
            signature.clone(),
        )
    }

    fn leaf(&self, leaf: &PyValue<'_, 'py>, kind: LeafKind) -> PyResult<(LeafWire, Option<Owner>)> {
        let obj = &leaf.obj;
        match kind {
            LeafKind::Handle => {
                // A wrapper's `_to_pyhandle` clones the key into a Python
                // object that releases it as usual; the wire takes a clone of
                // its own.
                let handle = if obj.is_instance_of::<BamlPyHandle>() {
                    obj.clone()
                } else {
                    obj.call_method0("_to_pyhandle")?
                };
                let handle = handle.cast::<BamlPyHandle>()?.borrow();
                let handle_type = i32::try_from(handle.handle_type)
                    .map_err(|_| PyTypeError::new_err("handle type does not fit the wire"))?;
                let (key, owner) = match WireHandle::from_parts(handle.handle_key, handle_type)
                    .map_err(|error| PyTypeError::new_err(error.to_string()))?
                {
                    // A host value is the engine's reference to the host's
                    // own object: the key goes as it is, and nothing is
                    // cloned or owed.
                    WireHandle::Host(key, _) => (key.get(), None),
                    WireHandle::Engine(_) => {
                        let cloned =
                            crate::py_handle::handle_clone(handle.handle_key, "inbound encode")?;
                        let cloned = TableKey::new(cloned)
                            .ok_or_else(|| PyTypeError::new_err("a cloned handle key is 0"))?;
                        (cloned.get(), Some(Owner::Handle(cloned)))
                    }
                };
                Ok((LeafWire::Handle(BamlHandle { key, handle_type }), owner))
            }
            LeafKind::Media(kind) => {
                let payload: BamlValueMedia = match kind {
                    MediaKind::Image => obj
                        .cast::<media::BamlImage>()?
                        .borrow()
                        .wire_payload(self.py)?,
                    MediaKind::Audio => obj
                        .cast::<media::BamlAudio>()?
                        .borrow()
                        .wire_payload(self.py)?,
                    MediaKind::Video => obj
                        .cast::<media::BamlVideo>()?
                        .borrow()
                        .wire_payload(self.py)?,
                    MediaKind::Pdf => obj
                        .cast::<media::BamlPdf>()?
                        .borrow()
                        .wire_payload(self.py)?,
                    MediaKind::Generic => {
                        return Err(PyTypeError::new_err(
                            "a Python media value always has a kind",
                        ));
                    }
                };
                Ok((LeafWire::Media(payload), None))
            }
            LeafKind::Prompt => {
                let prompt = leaf
                    .prompt_data()?
                    .ok_or_else(|| PyTypeError::new_err("a prompt leaf lost its prompt"))?;
                let encoded = message_bytes(&prompt.call_method0("wire_copy")?)?;
                let ast = BamlValuePromptAst::decode(encoded.as_slice())
                    .map_err(|error| PyTypeError::new_err(error.to_string()))?;
                Ok((LeafWire::Prompt(ast), None))
            }
            LeafKind::TypeValue => {
                // The type as the wrapper holds it; the wire takes fresh
                // references to its runtime declarations at commit.
                let ty = self.wire_ty(&obj.call_method0("_wire_copy")?)?;
                Ok((LeafWire::TypeValue(ty), None))
            }
        }
    }

    fn reference(&self, declaration: &bex_project::Handle) -> PyResult<TableKey> {
        Ok(HANDLE_TABLE.insert(CffiHandleTableEntry::BexHeapHandle(declaration.clone())))
    }

    fn release(&self, owner: Owner) {
        match owner {
            Owner::Callable(key) => crate::host_value::release_registered(key),
            // Best effort: a key the engine never saw has no other owner.
            Owner::Handle(key) => {
                let _ = handle_cffi::release_handle(key.get());
            }
        }
    }
}

pub(crate) struct PyValue<'a, 'py> {
    obj: Bound<'py, PyAny>,
    cx: &'a Cx<'py>,
}

impl<'py> PyValue<'_, 'py> {
    fn is_a(&self, class: &Bound<'py, PyAny>) -> bool {
        self.obj.is_instance(class).unwrap_or(false)
    }

    /// The leaf this object is, if it is one.
    fn leaf_kind(&self) -> PyResult<Option<LeafKind>> {
        let obj = &self.obj;
        if obj.is_instance_of::<BamlPyHandle>()
            || self.cx.handle_wrappers.iter().any(|class| self.is_a(class))
        {
            return Ok(Some(LeafKind::Handle));
        }
        if obj.is_instance_of::<media::BamlImage>() {
            return Ok(Some(LeafKind::Media(MediaKind::Image)));
        }
        if obj.is_instance_of::<media::BamlAudio>() {
            return Ok(Some(LeafKind::Media(MediaKind::Audio)));
        }
        if obj.is_instance_of::<media::BamlVideo>() {
            return Ok(Some(LeafKind::Media(MediaKind::Video)));
        }
        if obj.is_instance_of::<media::BamlPdf>() {
            return Ok(Some(LeafKind::Media(MediaKind::Pdf)));
        }
        if self.is_a(&self.cx.baml_type) {
            return Ok(Some(LeafKind::TypeValue));
        }
        if self.prompt_data()?.is_some() {
            return Ok(Some(LeafKind::Prompt));
        }
        Ok(None)
    }

    /// The `_PortablePromptAst` this object is or carries: itself, or the
    /// `_data` of the generated `ai.Prompt` shell.
    fn prompt_data(&self) -> PyResult<Option<Bound<'py, PyAny>>> {
        if self.is_a(&self.cx.prompt_ast) {
            return Ok(Some(self.obj.clone()));
        }
        if !self.cx.is_model(&self.obj) {
            return Ok(None);
        }
        let private = self.obj.getattr("__pydantic_private__")?;
        if !private.is_none()
            && let Ok(data) = private.get_item("_data")
            && data.is_instance(&self.cx.prompt_ast)?
        {
            return Ok(Some(data));
        }
        for (wire_name, value) in self.model_fields()? {
            if wire_name == "_data" && value.is_instance(&self.cx.prompt_ast)? {
                return Ok(Some(value));
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
            // The generator records a renamed field's BAML name in `alias`;
            // a user's `serialization_alias` is for their own JSON, not ours.
            let wire_name = match infos.get_item(&name)? {
                Some(info) => {
                    let alias = info.getattr("alias")?;
                    if alias.is_none() {
                        name
                    } else {
                        alias.extract()?
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

    /// A pydantic instance of a class the typemap knows, with the type
    /// arguments pydantic's generic metadata gives it.
    fn model_shape(&self) -> PyResult<HostShape<'_, Self>> {
        let Some(class) = self.cx.baml_name(&self.obj.get_type().into_any())? else {
            return Ok(HostShape::Other);
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
                            self.cx.wire_ty(&arg.call_method0("_wire_copy")?)
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
        let cx = self.cx;
        let fields = self
            .model_fields()?
            .into_iter()
            .map(move |(name, value)| Ok((name, cx.value(value))));
        Ok(HostShape::Instance {
            class,
            type_args,
            fields: Box::new(fields),
        })
    }

    /// An `Enum` member of an enum the typemap knows, by its variant.
    fn enum_shape(&self) -> PyResult<HostShape<'_, Self>> {
        let Some(enumeration) = self.cx.baml_name(&self.obj.get_type().into_any())? else {
            return Ok(HostShape::Other);
        };
        let value = self.obj.getattr("value")?;
        let Ok(variant) = value.extract::<String>() else {
            return Err(PyTypeError::new_err(format!(
                "cannot encode enum member {}: BAML enum values are strings",
                self.describe()
            )));
        };
        Ok(HostShape::EnumMember {
            enumeration,
            variant,
        })
    }

    /// Python's `int`: the VM's integer when it fits, a `bigint` beyond.
    fn int_scalar(&self) -> PyResult<Scalar> {
        if let Ok(int) = self.obj.extract::<i64>()
            && let Some(int) = Int63::new(int)
        {
            return Ok(Scalar::Int(int));
        }
        // `int.__format__` itself, not the object's: a subclass must not get
        // to spell a different number.
        let hex: String = self
            .cx
            .py
            .get_type::<PyInt>()
            .getattr("__format__")?
            .call1((&self.obj, "x"))?
            .extract()?;
        BigInt::parse_bytes(hex.as_bytes(), 16)
            .map(Scalar::BigInt)
            .ok_or_else(|| PyTypeError::new_err("Python int did not format as base sixteen"))
    }
}

impl<'py> HostValue for PyValue<'_, 'py> {
    type Error = PyErr;

    const RULES: Rules = Rules::Python;

    fn shape(&self) -> PyResult<HostShape<'_, Self>> {
        let obj = &self.obj;
        if obj.is_none() {
            return Ok(HostShape::Null);
        }
        if let Some(kind) = self.leaf_kind()? {
            return Ok(HostShape::Leaf(kind));
        }
        // An enum member is a variant, whatever it mixes in.
        if self.cx.is_enum(obj) {
            return self.enum_shape();
        }
        if let Ok(bool) = obj.cast::<PyBool>() {
            return Ok(HostShape::Scalar(Scalar::Bool(bool.is_true())));
        }
        if obj.is_instance_of::<PyInt>() {
            return self.int_scalar().map(HostShape::Scalar);
        }
        if let Ok(float) = obj.cast::<PyFloat>() {
            return Ok(HostShape::Scalar(Scalar::Float(float.value())));
        }
        if obj.is_instance_of::<PyString>() {
            return Ok(HostShape::Scalar(Scalar::String));
        }
        if obj.is_instance_of::<PyBytes>() || obj.is_instance_of::<PyByteArray>() {
            return Ok(HostShape::Scalar(Scalar::Bytes));
        }
        let cx = self.cx;
        if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
            // Through the protocol, not the C-level items, so a subclass that
            // overrides `__iter__` is read the way Python reads it, and one
            // item at a time, so an endless `__iter__` yields only what the
            // walker's budget lets it read.
            let items = obj.try_iter()?.map(move |item| Ok(cx.value(item?)));
            return Ok(HostShape::List(Box::new(items)));
        }
        if obj.cast::<PyDict>().is_ok() {
            // Through Python's own `items()` iterator, not PyO3's: reading a
            // value runs host code (a list subclass's `__iter__`, a model
            // property) that may mutate this dict, and Python answers that
            // with a `RuntimeError` the walker reports as the host's error,
            // where PyO3's iterator would panic.
            let items = obj.call_method0("items")?.try_iter()?.map(move |item| {
                let (key, value) = item?.extract::<(Bound<'py, PyAny>, Bound<'py, PyAny>)>()?;
                Ok((cx.value(key), cx.value(value)))
            });
            return Ok(HostShape::Map(Box::new(items)));
        }
        // A type metadata row is a plain record of five attributes.
        if self.is_a(&self.cx.metadata_row) {
            let py = self.cx.py;
            let entries = ["ty", "alias", "description", "docstring", "other"]
                .iter()
                .map(move |name| {
                    Ok((
                        cx.value(PyString::new(py, name).into_any()),
                        cx.value(obj.getattr(*name)?),
                    ))
                });
            return Ok(HostShape::Map(Box::new(entries)));
        }
        if self.cx.is_model(obj) {
            return self.model_shape();
        }
        if obj.is_instance(&self.cx.class_instance)? {
            let class: String = obj.getattr("class_name")?.extract()?;
            let fields = obj.getattr("fields")?.cast_into::<PyDict>()?;
            let fields = fields.call_method0("items")?.try_iter()?.map(move |item| {
                let (name, value) = item?.extract::<(String, Bound<'py, PyAny>)>()?;
                Ok((name, cx.value(value)))
            });
            return Ok(HostShape::Instance {
                class: TypeName::from_dotted_path(&class),
                type_args: None,
                fields: Box::new(fields),
            });
        }
        // Classes are callable but are types, not functions.
        if obj.is_callable() && !obj.is_instance_of::<PyType>() {
            return Ok(HostShape::Callable);
        }
        Ok(HostShape::Other)
    }

    fn text(&self) -> PyResult<Cow<'_, str>> {
        self.obj.cast::<PyString>()?.to_str().map(Cow::Borrowed)
    }

    fn bytes(&self) -> PyResult<Cow<'_, [u8]>> {
        if let Ok(bytes) = self.obj.cast::<PyBytes>() {
            return Ok(Cow::Borrowed(bytes.as_bytes()));
        }
        // A bytearray is mutable, so its buffer is copied.
        Ok(Cow::Owned(self.obj.cast::<PyByteArray>()?.to_vec()))
    }

    /// The type's name and, for a scalar, its text; a container by its
    /// length. Bounded in work as in length: no user `__repr__` runs here.
    fn describe(&self) -> String {
        const LIMIT: usize = 48;
        let obj = &self.obj;
        let type_name = obj
            .get_type()
            .name()
            .map(|name| name.to_string())
            .unwrap_or_else(|_| "object".to_string());
        let shown = if obj.is_none() {
            String::new()
        } else if let Ok(text) = obj.cast::<PyString>() {
            let text = text.to_string_lossy();
            let mut shown: String = text.chars().take(LIMIT).collect();
            if shown.chars().count() < text.chars().count() {
                shown.push('…');
            }
            format!("{shown:?}")
        } else if obj.is_instance_of::<PyBool>()
            || obj.is_instance_of::<PyInt>()
            || obj.is_instance_of::<PyFloat>()
        {
            let mut shown: String = obj
                .str()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .chars()
                .take(LIMIT)
                .collect();
            if shown.chars().count() == LIMIT {
                shown.push('…');
            }
            shown
        } else if let Ok(len) = obj.len() {
            format!("of length {len}")
        } else {
            String::new()
        };
        if shown.is_empty() {
            type_name
        } else {
            format!("{type_name} {shown}")
        }
    }
}
