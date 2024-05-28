#[macro_export]
macro_rules! lang_wrapper {
    ($name:ident, $type:ty, clone_safe $(, $attr_name:ident : $attr_type:ty = $default:expr)*) => {
        #[pyo3::prelude::pyclass]
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

    ($name:ident, $type:ty, thread_safe $(, $attr_name:ident : $attr_type:ty)*) => {
        #[pyo3::prelude::pyclass]
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
        #[pyo3::prelude::pyclass]
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
        #[pyo3::prelude::pyclass]
        pub struct $name {
            pub(crate) inner: $type,
            $($attr_name: $attr_type),*
        }
    };
}
