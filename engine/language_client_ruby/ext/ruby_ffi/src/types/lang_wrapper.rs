#[macro_export]
macro_rules! lang_wrapper {

    ($name:ident, $wrap_name:expr, $type:ty, sync_thread_safe $(, $attr_name:ident : $attr_type:ty)*) => {
        #[magnus::wrap(class = $wrap_name, free_immediately, size)]
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

}
