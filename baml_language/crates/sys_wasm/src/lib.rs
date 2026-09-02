//! Narrow Web implementations of the BAML `SysOps` table.
//!
//! JavaScript platform calls reuse the same `HOST_VALUE_CALLABLE` registry,
//! protobuf wire format, and completion table as user-provided host callables.

use std::{cell::RefCell, sync::Arc};

mod host_value;
mod send_wrapper;
mod web_sysops;

use bex_project::HostValueArc;
use host_value::retain_host_callable;
pub use host_value::{
    WasmHost, complete_host_call, mint_host_value_key, register_host_callable,
    register_host_value_release_callback, release_host_callable, test_fire_host_release,
    test_host_callable_count, test_host_release_callback_installed, test_in_flight_host_call_count,
    test_missing_host_callable_error, test_sync_pending_host_callable_error, with_web_sync_mode,
};
#[doc(hidden)]
pub use send_wrapper::{SendFuture, SendWrapper};
use web_sysops::{WebFs, WebHttp, WebTime};

#[derive(Clone)]
struct WebSysopConfig {
    fetch: Arc<HostValueArc>,
    read_file_sync: Arc<HostValueArc>,
}

thread_local! {
    static WEB_SYSOP_CONFIG: RefCell<Option<WebSysopConfig>> = const { RefCell::new(None) };
}

/// Configure the bridge-owned callables used by the Web `SysOp` table.
pub fn configure_web_sysops(fetch_key: u64, read_file_sync_key: u64) -> Result<(), String> {
    let config = WebSysopConfig {
        fetch: retain_host_callable(fetch_key)?,
        read_file_sync: retain_host_callable(read_file_sync_key)?,
    };
    WEB_SYSOP_CONFIG.with(|slot| {
        slot.borrow_mut().replace(config);
    });
    Ok(())
}

/// Build the Web capability surface: fetch, readFileSync, user host callables,
/// and the wall clock used by `baml.time.Instant.now`.
pub fn build() -> Result<sys_ops::SysOps, String> {
    let config = WEB_SYSOP_CONFIG
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| {
            "configure_web_sysops must be called before initializing the Web runtime".to_string()
        })?;
    let host = Arc::new(WasmHost::direct());
    let http = Arc::new(WebHttp::new(host.clone(), config.fetch));
    let fs = Arc::new(WebFs::new(host.clone(), config.read_file_sync));

    Ok(sys_ops::SysOpsBuilder::new()
        .with_http_fetch_instance(http)
        .with_fs_read_instance(fs)
        .with_host_instance(host)
        .with_time_instance(Arc::new(WebTime))
        .build())
}
