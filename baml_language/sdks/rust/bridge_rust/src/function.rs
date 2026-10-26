//! Owned BAML closures returned through the native handle table.

use std::{marker::PhantomData, sync::Arc};

use crate::{
    BamlValue, DecodeError, Error, SdkError,
    baml_value::internal::{__BamlValuePrivate, EncodeError, Encoder},
    wire::{self, baml_outbound_value::Value as Out},
};

pub(crate) struct FunctionHandle {
    pub(crate) key: u64,
    #[cfg(test)]
    pub(crate) release: Option<Arc<dyn Fn(u64) + Send + Sync>>,
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

/// One parameter of a BAML closure, as its specialized function type
/// declares it: the name an argument is passed under (`arg<index>` for a
/// nameless one, as the engine names it) and the type an argument is stated
/// as.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct Parameter {
    name: String,
    ty: wire::BamlTy,
}

/// A typed BAML closure. Rust cannot implement the `Fn` traits for user
/// types on stable Rust, so invocation is exposed as [`call`](Self::call)
/// and [`call_async`](Self::call_async).
pub struct BamlFunction<Args, Ret, Throws> {
    pub(crate) handle: Arc<FunctionHandle>,
    parameters: Arc<[Parameter]>,
    marker: PhantomData<fn(Args) -> Result<Ret, Throws>>,
}

impl<Args, Ret, Throws> Clone for BamlFunction<Args, Ret, Throws> {
    fn clone(&self) -> Self {
        Self {
            handle: Arc::clone(&self.handle),
            parameters: Arc::clone(&self.parameters),
            marker: PhantomData,
        }
    }
}

impl<Args, Ret, Throws> std::fmt::Debug for BamlFunction<Args, Ret, Throws> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BamlFunction")
            .field(
                "parameters",
                &self
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>(),
            )
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
        self.call_with_options(args, crate::invocation::InvocationOptions::default())
    }

    pub fn call_with_options(
        &self,
        args: Args,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Ret, Error<Throws>> {
        self.check_arity()?;
        crate::runtime::invoke_handle_sync_with_options(
            self.handle.key,
            |arguments| args.encode(&self.parameters, arguments),
            baml.into(),
        )
    }
    pub async fn call_async_with_options(
        &self,
        args: Args,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Ret, Error<Throws>> {
        self.check_arity()?;
        crate::runtime::invoke_handle_with_options(
            self.handle.key,
            |arguments| args.encode(&self.parameters, arguments),
            baml.into(),
        )
        .await
    }

    /// Invoke the closure asynchronously.
    pub async fn call_async(&self, args: Args) -> Result<Ret, Error<Throws>> {
        self.call_async_with_options(args, crate::invocation::InvocationOptions::default())
            .await
    }

    /// The closure's specialized function type takes as many parameters as
    /// the Rust signature it was decoded under.
    fn check_arity(&self) -> Result<(), Error<Throws>> {
        if self.parameters.len() == Args::ARITY {
            Ok(())
        } else {
            Err(Error::Sdk(SdkError::new(
                "BAML function handle arity mismatch",
            )))
        }
    }
}

/// Tuple conversion used by generated returned-function types.
#[doc(hidden)]
pub trait FunctionArgs: Sized {
    /// How many parameters the tuple fills.
    const ARITY: usize;
    /// Encode each argument as the parameter it fills, `parameters` being
    /// the closure's own, [`ARITY`](Self::ARITY) of them.
    fn encode(
        self,
        parameters: &[Parameter],
        arguments: &mut crate::encode::Arguments,
    ) -> Result<(), crate::encode::ArgumentError>;
    fn parameters() -> Vec<(wire::BamlTy, wire::BamlTyFunctionParamMode)>;
}

#[doc(hidden)]
pub trait FunctionArgument: Sized {
    /// Encode a supplied value as the argument `parameter`, stated as the
    /// parameter's specialized type; an omitted optional sends nothing.
    fn encode(
        &self,
        parameter: &Parameter,
        arguments: &mut crate::encode::Arguments,
    ) -> Result<(), crate::encode::ArgumentError>;
    /// Return the wire type and required/optional mode for this tuple slot.
    fn parameter() -> (wire::BamlTy, wire::BamlTyFunctionParamMode);
}

impl<T: BamlValue> FunctionArgument for T {
    fn encode(
        &self,
        parameter: &Parameter,
        arguments: &mut crate::encode::Arguments,
    ) -> Result<(), crate::encode::ArgumentError> {
        arguments.value(&parameter.name, self, &parameter.ty)
    }

    fn parameter() -> (wire::BamlTy, wire::BamlTyFunctionParamMode) {
        (T::baml_ty(), wire::BamlTyFunctionParamMode::Required)
    }
}

impl<T: BamlValue> FunctionArgument for crate::OptionalArg<T> {
    fn encode(
        &self,
        parameter: &Parameter,
        arguments: &mut crate::encode::Arguments,
    ) -> Result<(), crate::encode::ArgumentError> {
        arguments.optional(&parameter.name, self, &parameter.ty)
    }

    fn parameter() -> (wire::BamlTy, wire::BamlTyFunctionParamMode) {
        (T::baml_ty(), wire::BamlTyFunctionParamMode::Optional)
    }
}

impl FunctionArgs for () {
    const ARITY: usize = 0;

    fn encode(
        self,
        parameters: &[Parameter],
        _arguments: &mut crate::encode::Arguments,
    ) -> Result<(), crate::encode::ArgumentError> {
        debug_assert!(parameters.is_empty(), "the caller checks the arity");
        Ok(())
    }

    fn parameters() -> Vec<(wire::BamlTy, wire::BamlTyFunctionParamMode)> {
        Vec::new()
    }
}

macro_rules! impl_function_args {
    ($count:expr; $(($type:ident, $value:ident, $index:tt)),+ $(,)?) => {
        impl<$($type: FunctionArgument),+> FunctionArgs for ($($type,)+) {
            const ARITY: usize = $count;

            fn encode(
                self,
                parameters: &[Parameter],
                arguments: &mut crate::encode::Arguments,
            ) -> Result<(), crate::encode::ArgumentError> {
                debug_assert_eq!(parameters.len(), $count, "the caller checks the arity");
                let ($($value,)+) = self;
                $($value.encode(&parameters[$index], arguments)?;)+
                Ok(())
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
    fn to_baml(&self, encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        Ok(wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::Handle(wire::BamlHandle {
                key: encoder.clone_handle(self.handle.key)?,
                handle_type: wire::BamlHandleType::FunctionRef as i32,
            })),
        })
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
        // The type may leave a parameter nameless (a closure BAML hands to a
        // callback does); the bridge partitions a call through the handle by
        // this same type, naming such a parameter `arg<index>`. An argument
        // is stated as the parameter's specialized type, which can be
        // narrower than the Rust type spells (`"a"[]` for a `Vec<String>`).
        let parameters = function_type
            .params
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                let ty = parameter.ty.ok_or(DecodeError::WrongType {
                    expected: "function",
                    got: "function type with an untyped parameter",
                })?;
                Ok(Parameter {
                    name: parameter
                        .name
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| format!("arg{index}")),
                    ty,
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;
        Ok(Self {
            handle: owned_handle,
            parameters: parameters.into(),
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
            parameters: Arc::from([]),
            marker: std::marker::PhantomData,
        };
        let cloned = function.clone();
        assert!(Arc::ptr_eq(&function.handle, &cloned.handle));
    }

    fn parameter(name: &str) -> super::Parameter {
        super::Parameter {
            name: name.to_string(),
            ty: <i64 as crate::baml_value::internal::__BamlValuePrivate>::baml_ty(),
        }
    }

    #[test]
    fn tuple_arguments_know_their_arity() {
        assert_eq!(<() as FunctionArgs>::ARITY, 0);
        assert_eq!(<(i64,) as FunctionArgs>::ARITY, 1);
        assert_eq!(<(i64, i64) as FunctionArgs>::ARITY, 2);
    }

    #[test]
    fn unset_optional_arguments_are_omitted() {
        let mut arguments = crate::encode::Arguments::new();
        (crate::OptionalArg::<i64>::Unset,)
            .encode(&[parameter("value")], &mut arguments)
            .unwrap();
        assert!(arguments.into_parts().0.is_empty());
        let mut arguments = crate::encode::Arguments::new();
        (crate::OptionalArg::Set(7_i64),)
            .encode(&[parameter("value")], &mut arguments)
            .unwrap();
        assert_eq!(arguments.into_parts().0.len(), 1);
    }

    /// An argument is stated as the parameter's specialized type, not the
    /// Rust type it is held as: a `Vec<String>` filling `"a"[]` states
    /// `"a"[]`.
    #[test]
    fn arguments_are_stated_as_the_specialized_parameter() {
        use crate::baml_value::internal::{list_ty, literal_string_ty};
        let tags = list_ty(literal_string_ty("a"));
        let mut arguments = crate::encode::Arguments::new();
        (vec!["a".to_string()],)
            .encode(
                &[super::Parameter {
                    name: "tags".to_string(),
                    ty: tags.clone(),
                }],
                &mut arguments,
            )
            .unwrap();
        let (kwargs, _) = arguments.into_parts();
        assert_eq!(kwargs[0].value.as_ref().unwrap().value_type, Some(tags));
    }
}
