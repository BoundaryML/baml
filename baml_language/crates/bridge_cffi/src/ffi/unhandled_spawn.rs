use std::sync::Mutex;

use once_cell::sync::OnceCell;

use super::super::api::BamlUnhandledSpawnErrorCallback;

static CALLBACK: OnceCell<BamlUnhandledSpawnErrorCallback> = OnceCell::new();
static PENDING: Mutex<Vec<(Vec<u8>, bool)>> = Mutex::new(Vec::new());

#[unsafe(no_mangle)]
pub extern "C" fn register_unhandled_spawn_error_callback(
    callback: BamlUnhandledSpawnErrorCallback,
) {
    if CALLBACK.set(callback).is_err() {
        return;
    }
    let pending = std::mem::take(
        &mut *PENDING
            .lock()
            .expect("unhandled spawn callback queue poisoned"),
    );
    for (content, cancelled) in pending {
        callback(content.as_ptr().cast(), content.len(), i32::from(cancelled));
    }
}

pub fn dispatch(content: Vec<u8>, cancelled: bool) {
    if let Some(callback) = CALLBACK.get() {
        callback(content.as_ptr().cast(), content.len(), i32::from(cancelled));
    } else {
        PENDING
            .lock()
            .expect("unhandled spawn callback queue poisoned")
            .push((content, cancelled));
    }
}
