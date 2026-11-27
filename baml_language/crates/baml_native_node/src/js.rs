//! A thin layer over the Node-API C functions: the few the addon uses, each
//! taking the `env` of the call it is made in.
//!
//! Every function here is called on Node's main thread, inside a call from
//! JavaScript, with `env` and the `napi_value`s of that call.

use std::{ffi::c_void, ptr};

use napi_sys as sys;
pub(crate) use sys::{napi_env as Env, napi_ref as Ref, napi_value as Value};

/// A JavaScript exception thrown by code the addon called (a constructor, a
/// getter): it is taken out of the engine, and rethrown or delivered later.
pub(crate) struct Thrown(pub Value);

pub(crate) type JsResult<T> = Result<T, Thrown>;

/// The pending exception, taken out of the engine.
pub(crate) fn take_exception(env: Env) -> Thrown {
    let mut value = ptr::null_mut();
    // SAFETY: an env of the current call.
    unsafe { sys::napi_get_and_clear_last_exception(env, &raw mut value) };
    Thrown(value)
}

/// `Ok` when `status` is `napi_ok`, the pending exception otherwise (or a new
/// error when nothing is pending).
#[track_caller]
fn check(env: Env, status: sys::napi_status) -> JsResult<()> {
    if status == sys::Status::napi_ok {
        return Ok(());
    }
    let mut pending = false;
    // SAFETY: as above.
    unsafe { sys::napi_is_exception_pending(env, &raw mut pending) };
    if pending {
        Err(take_exception(env))
    } else {
        Err(Thrown(error(
            env,
            None,
            &format!(
                "Node-API call failed with status {status} (baml_native_node, {})",
                std::panic::Location::caller()
            ),
        )))
    }
}

/// A Rust index as a JavaScript array index. JavaScript arrays hold fewer
/// than 2^32 elements, and so do the interface's lists.
pub(crate) fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// A JavaScript number that is a whole number from 0 to 2^53, as an
/// integer: call ids, export numbers, modes.
pub(crate) fn whole(env: Env, value: Value) -> Option<u64> {
    let n = get_f64(env, value);
    if n >= 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_992.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "whole, not negative and at most 2^53: exact"
        )]
        Some(n as u64)
    } else {
        None
    }
}

pub(crate) fn undefined(env: Env) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_get_undefined(env, &raw mut value) };
    value
}

pub(crate) fn null(env: Env) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_get_null(env, &raw mut value) };
    value
}

pub(crate) fn boolean(env: Env, b: bool) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_get_boolean(env, b, &raw mut value) };
    value
}

pub(crate) fn number(env: Env, n: f64) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_create_double(env, n, &raw mut value) };
    value
}

pub(crate) fn string(env: Env, text: &str) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: `text` is its length of bytes.
    unsafe {
        sys::napi_create_string_utf8(
            env,
            text.as_ptr().cast(),
            isize::try_from(text.len()).unwrap_or(isize::MAX),
            &raw mut value,
        )
    };
    value
}

pub(crate) fn object(env: Env) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_create_object(env, &raw mut value) };
    value
}

pub(crate) fn array(env: Env, length: usize) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_create_array_with_length(env, length, &raw mut value) };
    value
}

/// A new `Error` with `message`, and `code` when given.
pub(crate) fn error(env: Env, code: Option<&str>, message: &str) -> Value {
    let code = code.map_or(ptr::null_mut(), |code| string(env, code));
    let message = string(env, message);
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_create_error(env, code, message, &raw mut value) };
    value
}

pub(crate) fn throw(env: Env, value: Value) -> Value {
    // SAFETY: as above.
    unsafe { sys::napi_throw(env, value) };
    ptr::null_mut()
}

#[track_caller]
pub(crate) fn set_element(env: Env, array: Value, index: u32, value: Value) -> JsResult<()> {
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_set_element(env, array, index, value)
    })
}

#[track_caller]
pub(crate) fn get_element(env: Env, array: Value, index: u32) -> JsResult<Value> {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_get_element(env, array, index, &raw mut value)
    })?;
    Ok(value)
}

#[track_caller]
pub(crate) fn set_property(env: Env, object: Value, key: Value, value: Value) -> JsResult<()> {
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_set_property(env, object, key, value)
    })
}

#[track_caller]
pub(crate) fn get_property(env: Env, object: Value, key: Value) -> JsResult<Value> {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_get_property(env, object, key, &raw mut value)
    })?;
    Ok(value)
}

#[track_caller]
pub(crate) fn get_named(env: Env, object: Value, name: &std::ffi::CStr) -> JsResult<Value> {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_get_named_property(env, object, name.as_ptr(), &raw mut value)
    })?;
    Ok(value)
}

pub(crate) fn type_of(env: Env, value: Value) -> sys::napi_valuetype {
    let mut kind = sys::ValueType::napi_undefined;
    // SAFETY: as above.
    unsafe { sys::napi_typeof(env, value, &raw mut kind) };
    kind
}

pub(crate) fn is_array(env: Env, value: Value) -> bool {
    let mut result = false;
    // SAFETY: as above.
    unsafe { sys::napi_is_array(env, value, &raw mut result) };
    result
}

pub(crate) fn array_length(env: Env, value: Value) -> u32 {
    let mut length = 0;
    // SAFETY: as above.
    unsafe { sys::napi_get_array_length(env, value, &raw mut length) };
    length
}

pub(crate) fn get_f64(env: Env, value: Value) -> f64 {
    let mut n = 0.0;
    // SAFETY: as above.
    unsafe { sys::napi_get_value_double(env, value, &raw mut n) };
    n
}

pub(crate) fn get_bool(env: Env, value: Value) -> bool {
    let mut b = false;
    // SAFETY: as above.
    unsafe { sys::napi_get_value_bool(env, value, &raw mut b) };
    b
}

/// The text of a JavaScript string, into `buffer` (reused between calls).
pub(crate) fn get_string(env: Env, value: Value, buffer: &mut Vec<u8>) -> &str {
    let mut length = 0;
    // SAFETY: as above; the first call measures, the second copies into a
    // buffer of that length plus the terminator Node writes.
    unsafe {
        sys::napi_get_value_string_utf8(env, value, ptr::null_mut(), 0, &raw mut length);
        buffer.clear();
        buffer.resize(length + 1, 0);
        sys::napi_get_value_string_utf8(
            env,
            value,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &raw mut length,
        );
        buffer.truncate(length);
        std::str::from_utf8_unchecked(buffer)
    }
}

pub(crate) fn strict_equals(env: Env, a: Value, b: Value) -> bool {
    let mut result = false;
    // SAFETY: as above.
    unsafe { sys::napi_strict_equals(env, a, b, &raw mut result) };
    result
}

/// The own enumerable string keys of `object`, as an array.
#[track_caller]
pub(crate) fn keys(env: Env, object: Value) -> JsResult<Value> {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_get_all_property_names(
            env,
            object,
            sys::KeyCollectionMode::own_only,
            sys::KeyFilter::enumerable | sys::KeyFilter::skip_symbols,
            sys::KeyConversion::numbers_to_strings,
            &raw mut value,
        )
    })?;
    Ok(value)
}

#[track_caller]
pub(crate) fn new_instance(env: Env, constructor: Value, args: &[Value]) -> JsResult<Value> {
    let mut value = ptr::null_mut();
    // SAFETY: `args` is its length of values.
    check(env, unsafe {
        sys::napi_new_instance(env, constructor, args.len(), args.as_ptr(), &raw mut value)
    })?;
    Ok(value)
}

#[track_caller]
pub(crate) fn call(env: Env, function: Value, args: &[Value]) -> JsResult<Value> {
    let mut value = ptr::null_mut();
    // SAFETY: as above.
    check(env, unsafe {
        sys::napi_call_function(
            env,
            undefined(env),
            function,
            args.len(),
            args.as_ptr(),
            &raw mut value,
        )
    })?;
    Ok(value)
}

/// A strong reference: keeps `value` alive for the life of the process.
pub(crate) fn reference(env: Env, value: Value) -> Ref {
    let mut reference = ptr::null_mut();
    // SAFETY: as above.
    unsafe { sys::napi_create_reference(env, value, 1, &raw mut reference) };
    reference
}

pub(crate) fn deref(env: Env, reference: Ref) -> Value {
    let mut value = ptr::null_mut();
    // SAFETY: a live reference made in this env.
    unsafe { sys::napi_get_reference_value(env, reference, &raw mut value) };
    value
}

/// The `this`-less arguments of the current call, up to `N`.
pub(crate) fn arguments<const N: usize>(env: Env, info: sys::napi_callback_info) -> [Value; N] {
    let mut args = [ptr::null_mut(); N];
    let mut count = N;
    // SAFETY: `args` has room for `count` values.
    unsafe {
        sys::napi_get_cb_info(
            env,
            info,
            &raw mut count,
            args.as_mut_ptr(),
            ptr::null_mut(),
            ptr::null_mut::<*mut c_void>(),
        )
    };
    for arg in args.iter_mut().skip(count) {
        *arg = undefined(env);
    }
    args
}

/// Add a function named `name` to `exports`.
pub(crate) fn export(
    env: Env,
    exports: Value,
    name: &std::ffi::CStr,
    function: unsafe extern "C" fn(Env, sys::napi_callback_info) -> Value,
) {
    let mut value = ptr::null_mut();
    // SAFETY: a name and a callback with Node-API's signature.
    unsafe {
        sys::napi_create_function(
            env,
            name.as_ptr(),
            isize::try_from(name.count_bytes()).unwrap_or(isize::MAX),
            Some(function),
            ptr::null_mut(),
            &raw mut value,
        );
        sys::napi_set_named_property(env, exports, name.as_ptr(), value);
    }
}
