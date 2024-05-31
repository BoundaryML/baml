#[macro_export]
macro_rules! lang_wrapper {
    ($name:ident, $type:ty, clone_safe $(, $attr_name:ident : $attr_type:ty = $default:expr)*) => {
        #[napi_derive::napi]
        pub struct $name {
            pub(crate) inner: std::sync::Arc<$type>,
            $($attr_name: $attr_type),*
        }

        impl From<$type> for $name {
            fn from(inner: $type) -> Self {
                Self {
                    inner: std::sync::Arc::new(inner),
                    $($attr_name: $default),*
                }
            }
        }
    };

    ($name:ident, $type:ty, sync_thread_safe $(, $attr_name:ident : $attr_type:ty)*) => {
        #[napi_derive::napi]
        pub struct $name {
            pub(crate) inner: std::sync::Arc<std::sync::Mutex<$type>>,
            $($attr_name: $attr_type),*
        }

        impl From<$type> for $name {
            fn from(inner: $type) -> Self {
                Self {
                    inner: std::sync::Arc::new(std::sync::Mutex::new(inner)),
                    $($attr_name: Default::default()),*
                }
            }
        }


        impl From<std::sync::Arc<std::sync::Mutex<$type>>> for $name {
            fn from(inner: std::sync::Arc<std::sync::Mutex<$type>>) -> Self {
                Self {
                    inner,
                    $($attr_name: Default::default()),*
                }
            }
        }
    };

    ($name:ident, $type:ty, no_from, thread_safe $(, $attr_name:ident : $attr_type:ty)*) => {
        #[napi_derive::napi]
        pub struct $name {
            pub(crate) inner: std::sync::Arc<tokio::sync::Mutex<$type>>,
            $($attr_name: $attr_type),*
        }
    };

    ($name:ident, $type:ty, custom_finalize, no_from, thread_safe $(, $attr_name:ident : $attr_type:ty)*) => {
        #[napi_derive::napi(custom_finalize)]
        pub struct $name {
            pub(crate) inner: std::sync::Arc<tokio::sync::Mutex<$type>>,
            $($attr_name: $attr_type),*
        }
    };

    ($name:ident, $type:ty, thread_safe $(, $attr_name:ident : $attr_type:ty)*) => {
        #[napi_derive::napi]
        pub struct $name {
            pub(crate) inner: std::sync::Arc<tokio::sync::Mutex<$type>>,
            $($attr_name: $attr_type),*
        }

        impl From<$type> for $name {
            fn from(inner: $type) -> Self {
                Self {
                    inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
                    $($attr_name: Default::default()),*
                }
            }
        }
    };

    ($name:ident, $type:ty $(, $attr_name:ident : $attr_type:ty = $default:expr)*) => {
        #[napi_derive::napi]
        pub struct $name {
            pub(crate) inner: $type,
            $($attr_name: $attr_type),*
        }

        impl From<$type> for $name {
            fn from(inner: $type) -> Self {
                Self {
                    inner,
                    $($attr_name: $default),*
                }
            }
        }
    };

    ($name:ident, $type:ty, no_from $(, $attr_name:ident : $attr_type:ty)*) => {
        #[napi_derive::napi]
        pub struct $name {
            pub(crate) inner: $type,
            $($attr_name: $attr_type),*
        }
    };
}
