//! Target-neutral, encoded host registration operations. Preparation pins
//! borrowed capabilities synchronously; results use ordinary transfer receipts.

use std::{panic::AssertUnwindSafe, sync::Arc};

use baml_type::Name;
use bex_project::{
    Bex, BexExternalAdt, BexExternalValue, HostAdapterImplementation, HostAdapterType,
    HostAdapterTypeDescriptor, HostDeclaration, HostValueArc, TypeArgument,
};
use bridge_ctypes::{
    CffiHandleTableEntry, CffiHandleTableOptions, EncodedTransfer, HANDLE_TABLE, InboundTransfer,
    OutboundEncoder,
    baml_bridge::cffi::{
        BamlOutboundValue, CreateHostAdapterRequest, HostAdapterCallbackSlot, HostOperationRequest,
        HostOperationResult, ProjectInterfaceRequest, RegisterHostAdapterRequest,
        RegisteredHostAdapter, host_operation_request, host_operation_result,
    },
};
use futures::FutureExt;
use prost::Message;

use crate::error::BridgeError;

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::InvalidInvocation(message.into())
}

/// Owns all input leases before the native caller hands work to an executor.
pub enum PreparedHostOperation {
    Register(PreparedHostRegistration),
    Create(PreparedHostInstance),
    Project(PreparedInterfaceProjection),
}

pub fn prepare_operation(bytes: &[u8]) -> Result<PreparedHostOperation, BridgeError> {
    let request = HostOperationRequest::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    match request
        .operation
        .ok_or_else(|| invalid("host operation is required"))?
    {
        host_operation_request::Operation::Register(request) => {
            prepare_registration_request(request).map(PreparedHostOperation::Register)
        }
        host_operation_request::Operation::Create(request) => {
            prepare_instance_request(request).map(PreparedHostOperation::Create)
        }
        host_operation_request::Operation::Project(request) => {
            prepare_projection_request(request).map(PreparedHostOperation::Project)
        }
    }
}

fn operation_result(result: host_operation_result::Result) -> Vec<u8> {
    HostOperationResult {
        result: Some(result),
    }
    .encode_to_vec()
}

/// Admission failures and engine errors have identical structured decoding.
pub fn operation_error(error: BridgeError) -> EncodedTransfer<'static, Vec<u8>> {
    crate::baml_to_host::error_to_outbound_message(error)
        .map_payload(|error| operation_result(host_operation_result::Result::Failure(error)))
}

/// Registration never executes user bodies. Cancellation drops prepared input
/// and provisional output; receipt delivery remains the native adapter's job.
pub async fn execute_operation(
    runtime: Arc<dyn Bex>,
    prepared: PreparedHostOperation,
) -> EncodedTransfer<'static, Vec<u8>> {
    let result = AssertUnwindSafe(async move {
        match prepared {
            PreparedHostOperation::Register(prepared) => {
                register_encoded(runtime, prepared).await.map(|encoded| {
                    encoded.map_payload(|value| {
                        operation_result(host_operation_result::Result::Registered(value))
                    })
                })
            }
            PreparedHostOperation::Create(prepared) => create_instance_encoded(runtime, prepared)
                .await
                .map(|encoded| {
                    encoded.map_payload(|value| {
                        operation_result(host_operation_result::Result::Value(value))
                    })
                }),
            PreparedHostOperation::Project(prepared) => {
                project_encoded(runtime, prepared).await.map(|encoded| {
                    encoded.map_payload(|value| {
                        operation_result(host_operation_result::Result::Value(value))
                    })
                })
            }
        }
    })
    .catch_unwind()
    .await;
    match result {
        Ok(Ok(encoded)) => encoded,
        Ok(Err(error)) => operation_error(error),
        Err(panic) => crate::baml_to_host::panic_to_outbound_message(panic.as_ref())
            .map_payload(|error| operation_result(host_operation_result::Result::Failure(error))),
    }
}

// Last field of prepared host input: drain after its owned input fields drop,
// including cancellation before the engine ever acquires a heap permit.
struct ReleaseHostValues;
impl Drop for ReleaseHostValues {
    fn drop(&mut self) {
        bex_project::host_release_dispatch::drain();
    }
}

pub struct PreparedHostRegistration {
    descriptor: HostAdapterTypeDescriptor,
    type_args: Vec<TypeArgument>,
}

pub fn prepare_registration(bytes: &[u8]) -> Result<PreparedHostRegistration, BridgeError> {
    let request =
        RegisterHostAdapterRequest::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    prepare_registration_request(request)
}

fn prepare_registration_request(
    request: RegisterHostAdapterRequest,
) -> Result<PreparedHostRegistration, BridgeError> {
    if request.name.is_empty() {
        return Err(invalid("host adapter name is required"));
    }
    let implementations = request
        .implementations
        .into_iter()
        .map(|implementation| {
            let template = implementation
                .interface_template
                .ok_or_else(|| invalid("host adapter interface template is required"))?;
            let template = bridge_ctypes::proto_ty_to_runtime_ty(&template)?.to_frame_template();
            Ok(HostAdapterImplementation {
                interface: template.map_heads(&mut |head| HostDeclaration::Named(head.clone())),
                methods: implementation.methods.iter().map(Name::new).collect(),
            })
        })
        .collect::<Result<Vec<_>, BridgeError>>()?;
    let type_args = request
        .type_args
        .iter()
        .map(|argument| {
            if !argument.type_var.is_empty() {
                return Err(invalid("host adapter type arguments are positional"));
            }
            Ok(bridge_ctypes::proto_type_argument(argument)?)
        })
        .collect::<Result<Vec<_>, BridgeError>>()?;
    Ok(PreparedHostRegistration {
        descriptor: HostAdapterTypeDescriptor {
            name: Name::new(request.name),
            implementations,
        },
        type_args,
    })
}

pub async fn register_encoded(
    runtime: Arc<dyn Bex>,
    prepared: PreparedHostRegistration,
) -> Result<EncodedTransfer<'static, RegisteredHostAdapter>, BridgeError> {
    let registration = Arc::new(
        runtime
            .register_host_adapter(prepared.descriptor, prepared.type_args)
            .await?,
    );
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let class_type = encoder.encode_handle(CffiHandleTableEntry::BexHeapHandle(
        registration.class_type().clone(),
    ));
    let interface_types = registration
        .interfaces()
        .iter()
        .map(|ty| encoder.encode_handle(CffiHandleTableEntry::BexHeapHandle(ty.clone())))
        .collect();
    let callbacks = registration
        .callbacks()
        .iter()
        .map(|slot| HostAdapterCallbackSlot {
            implementation_index: slot.implementation_index,
            method: slot.method.to_string(),
        })
        .collect();
    let adapter_type = encoder.encode_handle(CffiHandleTableEntry::HostAdapterType(registration));
    Ok(encoder.finish(RegisteredHostAdapter {
        adapter_type: Some(adapter_type),
        class_type: Some(class_type),
        callbacks,
        interface_types,
    }))
}

pub struct PreparedHostInstance {
    registration: Arc<HostAdapterType>,
    receiver: Arc<HostValueArc>,
    callbacks: Vec<Arc<HostValueArc>>,
    release: ReleaseHostValues,
}

pub fn prepare_instance(bytes: &[u8]) -> Result<PreparedHostInstance, BridgeError> {
    let request =
        CreateHostAdapterRequest::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    prepare_instance_request(request)
}

fn prepare_instance_request(
    request: CreateHostAdapterRequest,
) -> Result<PreparedHostInstance, BridgeError> {
    let release = ReleaseHostValues;
    // Capture the complete parsed aggregate before even validating its target.
    let mut values = Vec::with_capacity(1 + request.callbacks.len());
    values.push(request.receiver.unwrap_or_default());
    values.extend(request.callbacks);
    let inputs = InboundTransfer::capture_values(&values, &HANDLE_TABLE);
    let entry = HANDLE_TABLE
        .resolve(request.adapter_type)
        .ok_or_else(|| invalid("host adapter type reference is closed"))?;
    let CffiHandleTableEntry::HostAdapterType(registration) = &*entry else {
        return Err(invalid("reference is not a host adapter registration"));
    };
    let registration = registration.clone();
    let mut items = inputs.decode_values(values)?.into_iter();
    let host = |value| match value {
        BexExternalValue::HostValue(value) => Ok(value),
        _ => Err(invalid(
            "host receiver and callbacks must be host registrations",
        )),
    };
    let receiver = host(items.next().expect("receiver slot was supplied"))?;
    let callbacks = items.map(host).collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedHostInstance {
        registration,
        receiver,
        callbacks,
        release,
    })
}

pub async fn create_instance_encoded(
    runtime: Arc<dyn Bex>,
    prepared: PreparedHostInstance,
) -> Result<EncodedTransfer<'static, BamlOutboundValue>, BridgeError> {
    let PreparedHostInstance {
        registration,
        receiver,
        callbacks,
        release: _release,
    } = prepared;
    let instance = runtime
        .create_host_adapter_instance(registration, receiver, callbacks)
        .await?;
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let value = encoder.encode(&BexExternalValue::Handle(instance))?;
    Ok(encoder.finish(value))
}

pub struct PreparedInterfaceProjection {
    value: BexExternalValue,
    expected: TypeArgument,
}

pub fn prepare_projection(bytes: &[u8]) -> Result<PreparedInterfaceProjection, BridgeError> {
    let request =
        ProjectInterfaceRequest::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    prepare_projection_request(request)
}

fn prepare_projection_request(
    request: ProjectInterfaceRequest,
) -> Result<PreparedInterfaceProjection, BridgeError> {
    let entry = HANDLE_TABLE
        .resolve(request.receiver)
        .ok_or_else(|| invalid("interface receiver is closed"))?;
    let value = BexExternalValue::try_from((*entry).clone()).map_err(invalid)?;
    let expected = request
        .interface_type
        .ok_or_else(|| invalid("interface type evidence is required"))?;
    if !expected.type_var.is_empty() {
        return Err(invalid("interface projection type is positional"));
    }
    let expected = bridge_ctypes::proto_type_argument(&expected)?;
    Ok(PreparedInterfaceProjection { value, expected })
}

pub async fn project_encoded(
    runtime: Arc<dyn Bex>,
    prepared: PreparedInterfaceProjection,
) -> Result<EncodedTransfer<'static, BamlOutboundValue>, BridgeError> {
    let view = runtime
        .project_interface(prepared.value, prepared.expected)
        .await?;
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let value = encoder.encode(&BexExternalValue::Adt(BexExternalAdt::Interface(view)))?;
    Ok(encoder.finish(value))
}
