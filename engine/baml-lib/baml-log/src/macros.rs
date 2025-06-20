#[macro_export]
macro_rules! blog {
    ($level:expr, $($arg:tt)*) => {
        $crate::log_internal(
            $level,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}

#[macro_export]
macro_rules! bfatal_once {
    ($($arg:tt)*) => {
        $crate::log_internal_once(
            $crate::Level::Fatal,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}

/// Log a message at the ERROR level
#[macro_export]
macro_rules! berror {
    ($($arg:tt)*) => {
        $crate::log_internal(
            $crate::Level::Error,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}

/// Log a message at the WARN level
#[macro_export]
macro_rules! bwarn {
    ($($arg:tt)*) => {
        $crate::log_internal(
            $crate::Level::Warn,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}

/// Log a message at the INFO level
#[macro_export]
macro_rules! binfo {
    ($($arg:tt)*) => {
        $crate::log_internal(
            $crate::Level::Info,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}

/// Log a message at the DEBUG level
#[macro_export]
macro_rules! bdebug {
    ($($arg:tt)*) => {
        $crate::log_internal(
            $crate::Level::Debug,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}

/// Log a message at the TRACE level
#[macro_export]
macro_rules! btrace {
    ($($arg:tt)*) => {
        $crate::log_internal(
            $crate::Level::Trace,
            &format!($($arg)*),
            Some(module_path!()),
            Some(file!()),
            Some(line!())
        )
    };
}
