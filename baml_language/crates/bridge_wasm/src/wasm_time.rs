//! WASM `baml.time` namespace implementation.
//!
//! `Instant.now()` reads the wall clock directly via `web_time`, which maps to
//! the browser's `Date.now()` / `performance.now()` on wasm targets. No JS
//! callback is needed, so `WasmTime` is a unit struct.
//!
//! Timezone-database operations (`system_timezone`, `_tz_offset_at`,
//! `_tz_to_instant`) delegate to the host JavaScript [`Temporal`] API, per
//! operation and with no caching, per BEP-021: there is no host filesystem on
//! wasm, and going through `Temporal` keeps the wasm build's view of tzdata
//! identical to the surrounding JS environment's. Hosts without `Temporal`
//! get `Unsupported`.
//!
//! [`Temporal`]: https://tc39.es/proposal-temporal/docs/

use std::sync::Arc;

use js_sys::{Array, BigInt as JsBigInt, Function, Object, Reflect};
use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, VmBamlError, owned};
use sys_types::BexHeap;
use wasm_bindgen::{JsCast, JsValue};

/// WASM implementation of the `baml.time` namespace.
pub(crate) struct WasmTime;

/// Temporal's representable-instant limit: ±100,000,000 days from the epoch,
/// in nanoseconds.
const TEMPORAL_LIMIT_NS: i128 = 8_640_000_000_000_000_000_000;

/// The host's `globalThis.Temporal`, or `Unsupported` if the JS environment
/// does not provide it.
fn temporal() -> Result<Object, VmBamlError> {
    Reflect::get(&js_sys::global(), &"Temporal".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<Object>().ok())
        .ok_or_else(|| VmBamlError::Unsupported {
            message: "the host JavaScript environment does not provide Temporal".to_string(),
        })
}

fn other(context: &str, e: &JsValue) -> VmBamlError {
    VmBamlError::DevOther {
        message: format!("{context}: {e:?}"),
    }
}

/// Converts an arbitrary-precision nanosecond value to a JS `BigInt`.
fn js_bigint(ns: &num_bigint::BigInt) -> Result<JsValue, VmBamlError> {
    JsBigInt::new(&JsValue::from_str(&ns.to_string()))
        .map(JsValue::from)
        .map_err(|e| other("constructing a JS BigInt", &e.into()))
}

/// Constructs `new Temporal.ZonedDateTime(epoch_ns, timezone)`. A constructor
/// throw is an unknown/invalid timezone identifier (the epoch argument is
/// range-checked by callers), reported as `None`.
fn new_zoned_date_time(
    temporal: &Object,
    epoch_ns: &JsValue,
    timezone: &str,
) -> Result<Option<Object>, VmBamlError> {
    let ctor: Function = Reflect::get(temporal, &"ZonedDateTime".into())
        .map_err(|e| other("accessing Temporal.ZonedDateTime", &e))?
        .dyn_into()
        .map_err(|e| other("Temporal.ZonedDateTime is not a constructor", &e))?;
    let args = Array::of2(epoch_ns, &JsValue::from_str(timezone));
    Ok(Reflect::construct(&ctor, &args)
        .ok()
        .and_then(|v| v.dyn_into::<Object>().ok()))
}

impl io::IoClassTimeInstant for WasmTime {
    fn now(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::time::Instant> {
        // Wall-clock time as nanoseconds since the UNIX epoch. Per the
        // `Instant.now()` contract, an unavailable/pre-epoch clock panics.
        let nanos = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .expect("system clock is set before the UNIX epoch")
            .as_nanos();
        SysOpOutput::ok(owned::time::Instant {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(nanos)),
        })
    }
}

impl io::IoNamespaceTime for WasmTime {
    fn system_timezone(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // Temporal.Now.timeZoneId()
        let result: Result<String, VmBamlError> = (|| {
            let temporal = temporal()?;
            let now = Reflect::get(&temporal, &"Now".into())
                .map_err(|e| other("accessing Temporal.Now", &e))?;
            let f: Function = Reflect::get(&now, &"timeZoneId".into())
                .map_err(|e| other("accessing Temporal.Now.timeZoneId", &e))?
                .dyn_into()
                .map_err(|e| other("Temporal.Now.timeZoneId is not a function", &e))?;
            f.call0(&now)
                .map_err(|e| other("calling Temporal.Now.timeZoneId", &e))?
                .as_string()
                .ok_or_else(|| VmBamlError::DevOther {
                    message: "Temporal.Now.timeZoneId did not return a string".to_string(),
                })
        })();
        match result {
            Ok(id) => SysOpOutput::ok(id),
            Err(e) => SysOpOutput::err(e),
        }
    }

    fn _tz_offset_at(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        timezone: String,
        at_ns: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<i64>> {
        let result: Result<Option<i64>, VmBamlError> = (|| {
            let temporal = temporal()?;
            // Saturate to Temporal's representable range: a timezone's offset
            // is constant beyond the last tzdb transition, so the boundary
            // instant resolves to the same offset as any farther one.
            let clamped = i128::try_from(&*at_ns)
                .unwrap_or(if at_ns.sign() == num_bigint::Sign::Minus {
                    i128::MIN
                } else {
                    i128::MAX
                })
                .clamp(-TEMPORAL_LIMIT_NS, TEMPORAL_LIMIT_NS);
            let epoch_ns = js_bigint(&num_bigint::BigInt::from(clamped))?;
            let Some(zdt) = new_zoned_date_time(&temporal, &epoch_ns, &timezone)? else {
                return Ok(None); // unknown timezone identifier
            };
            let offset = Reflect::get(&zdt, &"offsetNanoseconds".into())
                .map_err(|e| other("accessing offsetNanoseconds", &e))?
                .as_f64()
                .ok_or_else(|| VmBamlError::DevOther {
                    message: "offsetNanoseconds is not a number".to_string(),
                })?;
            #[allow(clippy::cast_possible_truncation)] // offsets are < ±24h in ns
            Ok(Some(offset as i64))
        })();
        match result {
            Ok(v) => SysOpOutput::ok(v),
            Err(e) => SysOpOutput::err(e),
        }
    }

    fn _tz_to_instant(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        timezone: String,
        civil_ns: Arc<num_bigint::BigInt>,
        disambiguation: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<Arc<num_bigint::BigInt>>> {
        let result: Result<Option<Arc<num_bigint::BigInt>>, VmBamlError> = (|| {
            let temporal = temporal()?;
            // Decode the civil reading (ns since 1970-01-01T00:00:00 as if
            // UTC) through a UTC ZonedDateTime, then re-locate the wall-clock
            // reading in the target timezone:
            //   new Temporal.ZonedDateTime(civil_ns, "UTC")
            //     .toPlainDateTime()
            //     .toZonedDateTime(timezone, { disambiguation })
            //     .epochNanoseconds
            let in_range = i128::try_from(&*civil_ns)
                .ok()
                .filter(|ns| ns.abs() <= TEMPORAL_LIMIT_NS);
            let Some(civil) = in_range else {
                return Err(VmBamlError::Io {
                    message: "civil time is outside the supported year range".to_string(),
                });
            };
            let epoch_ns = js_bigint(&num_bigint::BigInt::from(civil))?;
            let utc = new_zoned_date_time(&temporal, &epoch_ns, "UTC")?.ok_or_else(|| {
                VmBamlError::DevOther {
                    message: "Temporal rejected the UTC timezone".to_string(),
                }
            })?;
            let to_plain: Function = Reflect::get(&utc, &"toPlainDateTime".into())
                .map_err(|e| other("accessing toPlainDateTime", &e))?
                .dyn_into()
                .map_err(|e| other("toPlainDateTime is not a function", &e))?;
            let plain = to_plain
                .call0(&utc)
                .map_err(|e| other("calling toPlainDateTime", &e))?;
            let to_zoned: Function = Reflect::get(&plain, &"toZonedDateTime".into())
                .map_err(|e| other("accessing toZonedDateTime", &e))?
                .dyn_into()
                .map_err(|e| other("toZonedDateTime is not a function", &e))?;
            let options = Object::new();
            Reflect::set(
                &options,
                &"disambiguation".into(),
                &JsValue::from_str(&disambiguation),
            )
            .map_err(|e| other("setting disambiguation", &e))?;
            // A throw here is an unknown timezone identifier: the civil value
            // is already range-checked and the disambiguation modes the BAML
            // layer sends ("compatible"/"earlier"/"later") never reject.
            let Ok(zdt) = to_zoned.call2(&plain, &JsValue::from_str(&timezone), &options) else {
                return Ok(None);
            };
            let epoch: JsBigInt = Reflect::get(&zdt, &"epochNanoseconds".into())
                .map_err(|e| other("accessing epochNanoseconds", &e))?
                .dyn_into()
                .map_err(|e| other("epochNanoseconds is not a BigInt", &e))?;
            let decimal = epoch
                .to_string(10)
                .map_err(|e| other("stringifying epochNanoseconds", &e.into()))?;
            let parsed = num_bigint::BigInt::parse_bytes(String::from(decimal).as_bytes(), 10)
                .ok_or_else(|| VmBamlError::DevOther {
                    message: "could not parse epochNanoseconds".to_string(),
                })?;
            Ok(Some(Arc::new(parsed)))
        })();
        match result {
            Ok(v) => SysOpOutput::ok(v),
            Err(e) => SysOpOutput::err(e),
        }
    }
}
