//! Owned BAML closures returned through the native handle table.

use std::{marker::PhantomData, sync::Arc};

use crate::{
    BamlValue, DecodeError, Error, SdkError,
    baml_value::internal::__BamlValuePrivate,
    wire::{self, baml_outbound_value::Value as Out, inbound_map_entry::Key},
};

struct FunctionHandle {
    key: u64,
    #[cfg(test)]
    release: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}

impl Drop for FunctionHandle {
    fn drop(&mut self) {
        if self.key == 0 {
            return;
        }
        #[cfg(test)]
        if let Some(release) = &self.release {
            release(self.key);
            return;
        }
        if let Ok(api) = crate::capi::api() {
            #[expect(unsafe_code)]
            unsafe {
                (api.handle_release)(self.key);
            }
        }
    }
}

/// A typed BAML closure. Rust cannot implement the `Fn` traits for user
/// types on stable Rust, so invocation is exposed as [`call`](Self::call)
/// and [`call_async`](Self::call_async).
pub struct BamlFunction<Args, Ret, Throws> {
    handle: Arc<FunctionHandle>,
    parameter_names: Arc<[String]>,
    marker: PhantomData<fn(Args) -> Result<Ret, Throws>>,
}

impl<Args, Ret, Throws> Clone for BamlFunction<Args, Ret, Throws> {
    fn clone(&self) -> Self {
        Self {
            handle: Arc::clone(&self.handle),
            parameter_names: Arc::clone(&self.parameter_names),
            marker: PhantomData,
        }
    }
}

impl<Args, Ret, Throws> std::fmt::Debug for BamlFunction<Args, Ret, Throws> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BamlFunction")
            .field("parameters", &self.parameter_names)
            .finish_non_exhaustive()
    }
}

impl<Args, Ret, Throws> BamlFunction<Args, Ret, Throws>
where
    Args: FunctionArgs,
    Ret: BamlValue,
    Throws: BamlValue,
{
    /// Invoke the closure synchronously.
    pub fn call(&self, args: Args) -> Result<Ret, Error<Throws>> {
        let kwargs = args
            .into_kwargs(&self.parameter_names)
            .map_err(Error::Sdk)?;
        crate::runtime::invoke_handle_sync(self.handle.key, kwargs)
    }

    /// Invoke the closure asynchronously.
    pub async fn call_async(&self, args: Args) -> Result<Ret, Error<Throws>> {
        let kwargs = args
            .into_kwargs(&self.parameter_names)
            .map_err(Error::Sdk)?;
        crate::runtime::invoke_handle(self.handle.key, kwargs).await
    }
}

/// Tuple conversion used by generated returned-function types.
#[doc(hidden)]
pub trait FunctionArgs: Sized {
    fn into_kwargs(self, names: &[String]) -> Result<Vec<wire::InboundMapEntry>, SdkError>;
    fn parameters() -> Vec<(wire::BamlTy, wire::BamlTyFunctionParamMode)>;
}

#[doc(hidden)]
pub trait FunctionArgument: Sized {
    /// Encode a supplied value, or return `None` when an optional is omitted.
    fn into_baml_opt(self) -> Option<wire::InboundValue>;
    /// Return the wire type and required/optional mode for this tuple slot.
    fn parameter() -> (wire::BamlTy, wire::BamlTyFunctionParamMode);
}

impl<T: BamlValue> FunctionArgument for T {
    fn into_baml_opt(self) -> Option<wire::InboundValue> {
        Some(self.to_baml())
    }

    fn parameter() -> (wire::BamlTy, wire::BamlTyFunctionParamMode) {
        (T::baml_ty(), wire::BamlTyFunctionParamMode::Required)
    }
}

impl<T: BamlValue> FunctionArgument for crate::OptionalArg<T> {
    fn into_baml_opt(self) -> Option<wire::InboundValue> {
        self.to_baml_opt()
    }

    fn parameter() -> (wire::BamlTy, wire::BamlTyFunctionParamMode) {
        (T::baml_ty(), wire::BamlTyFunctionParamMode::Optional)
    }
}

impl FunctionArgs for () {
    fn into_kwargs(self, names: &[String]) -> Result<Vec<wire::InboundMapEntry>, SdkError> {
        if names.is_empty() {
            Ok(Vec::new())
        } else {
            Err(SdkError::new("BAML function handle arity mismatch"))
        }
    }

    fn parameters() -> Vec<(wire::BamlTy, wire::BamlTyFunctionParamMode)> {
        Vec::new()
    }
}

macro_rules! impl_function_args {
    ($count:expr; $(($type:ident, $value:ident, $index:tt)),+ $(,)?) => {
        impl<$($type: FunctionArgument),+> FunctionArgs for ($($type,)+) {
            fn into_kwargs(
                self,
                names: &[String],
            ) -> Result<Vec<wire::InboundMapEntry>, SdkError> {
                if names.len() != $count {
                    return Err(SdkError::new("BAML function handle arity mismatch"));
                }
                let ($($value,)+) = self;
                Ok(vec![$(
                    $value.into_baml_opt().map(|value| wire::InboundMapEntry {
                        key: Some(Key::StringKey(names[$index].clone())),
                        value: Some(value),
                    })
                ),+]
                .into_iter()
                .flatten()
                .collect())
            }

            fn parameters() -> Vec<(wire::BamlTy, wire::BamlTyFunctionParamMode)> {
                vec![$($type::parameter()),+]
            }
        }
    };
}

impl_function_args!(1; (A0, a0, 0));
impl_function_args!(2; (A0, a0, 0), (A1, a1, 1));
impl_function_args!(3; (A0, a0, 0), (A1, a1, 1), (A2, a2, 2));
impl_function_args!(4; (A0, a0, 0), (A1, a1, 1), (A2, a2, 2), (A3, a3, 3));
impl_function_args!(5; (A0, a0, 0), (A1, a1, 1), (A2, a2, 2), (A3, a3, 3), (A4, a4, 4));
impl_function_args!(6; (A0, a0, 0), (A1, a1, 1), (A2, a2, 2), (A3, a3, 3), (A4, a4, 4), (A5, a5, 5));
impl_function_args!(7; (A0, a0, 0), (A1, a1, 1), (A2, a2, 2), (A3, a3, 3), (A4, a4, 4), (A5, a5, 5), (A6, a6, 6));
impl_function_args!(8; (A0, a0, 0), (A1, a1, 1), (A2, a2, 2), (A3, a3, 3), (A4, a4, 4), (A5, a5, 5), (A6, a6, 6), (A7, a7, 7));

impl<Args, Ret, Throws> __BamlValuePrivate for BamlFunction<Args, Ret, Throws>
where
    Args: FunctionArgs,
    Ret: BamlValue,
    Throws: BamlValue,
{
    fn to_baml(&self) -> wire::InboundValue {
        let mut cloned = 0_u64;
        let api = crate::capi::api().expect("BAML runtime must be loaded");
        #[expect(unsafe_code)]
        let status = unsafe { (api.handle_clone)(self.handle.key, &raw mut cloned) };
        assert_eq!(status, 0, "failed to clone a BAML function handle");
        wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::Handle(wire::BamlHandle {
                key: cloned,
                handle_type: wire::BamlHandleType::FunctionRef as i32,
            })),
        }
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let value = crate::decode::unwrap(value);
        let got = crate::baml_value::wire_variant_kind(&value);
        let Some(Out::HandleValue(handle)) = value.value else {
            return Err(DecodeError::WrongType {
                expected: "function",
                got,
            });
        };
        if handle.key == 0 || handle.handle_type != wire::BamlHandleType::FunctionRef as i32 {
            return Err(DecodeError::WrongType {
                expected: "function",
                got: "handle",
            });
        }
        let owned_handle = Arc::new(FunctionHandle {
            key: handle.key,
            #[cfg(test)]
            release: None,
        });
        let Some(wire::baml_ty::Ty::Function(function_type)) = handle.ty.and_then(|ty| ty.ty)
        else {
            return Err(DecodeError::WrongType {
                expected: "function",
                got: "handle",
            });
        };
        let parameter_names = function_type
            .params
            .into_iter()
            .map(|parameter| parameter.name.filter(|name| !name.is_empty()))
            .collect::<Option<Vec<_>>>()
            .ok_or(DecodeError::WrongType {
                expected: "function with named parameters",
                got: "handle",
            })?;
        Ok(Self {
            handle: owned_handle,
            parameter_names: parameter_names.into(),
            marker: PhantomData,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::Function(Box::new(
                wire::BamlTyFunction {
                    generic_params: Vec::new(),
                    params: Args::parameters()
                        .into_iter()
                        .map(|(ty, mode)| wire::BamlTyFunctionParam {
                            name: None,
                            ty: Some(ty),
                            mode: mode as i32,
                        })
                        .collect(),
                    ret: Some(Box::new(Ret::baml_ty())),
                    throws: Some(Box::new(Throws::baml_ty())),
                },
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    use super::{BamlFunction, FunctionArgs, FunctionHandle};

    #[test]
    fn function_handle_drop_releases_exactly_its_key() {
        let released = Arc::new(AtomicU64::new(0));
        let capture = Arc::clone(&released);
        drop(FunctionHandle {
            key: 73,
            release: Some(Arc::new(move |key| {
                capture.store(key, Ordering::SeqCst);
            })),
        });
        assert_eq!(released.load(Ordering::SeqCst), 73);
    }

    #[test]
    fn cloned_functions_share_the_owned_handle() {
        let function = BamlFunction::<(), i64, Infallible> {
            handle: Arc::new(FunctionHandle {
                key: 0,
                release: None,
            }),
            parameter_names: Arc::from([]),
            marker: std::marker::PhantomData,
        };
        let cloned = function.clone();
        assert!(Arc::ptr_eq(&function.handle, &cloned.handle));
    }

    #[test]
    fn tuple_arguments_validate_arity_without_dispatching() {
        assert!(().into_kwargs(&[]).unwrap().is_empty());
        assert!((1_i64,).into_kwargs(&["value".to_string()]).is_ok());
        assert!(
            (1_i64, 2_i64)
                .into_kwargs(&["left".to_string()])
                .unwrap_err()
                .to_string()
                .contains("arity mismatch")
        );
        assert!(
            ().into_kwargs(&["unexpected".to_string()])
                .unwrap_err()
                .to_string()
                .contains("arity mismatch")
        );
    }

    #[test]
    fn unset_optional_arguments_are_omitted() {
        let kwargs = (crate::OptionalArg::<i64>::Unset,)
            .into_kwargs(&["value".to_string()])
            .unwrap();
        assert!(kwargs.is_empty());
        let kwargs = (crate::OptionalArg::Set(7_i64),)
            .into_kwargs(&["value".to_string()])
            .unwrap();
        assert_eq!(kwargs.len(), 1);
    }
}
