use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::types::Value;

use super::{
    BamlClassTimeInstant, BamlClassTimePlainDate, BamlClassTimePlainDateTime,
    BamlClassTimePlainTime, BamlClassTimeTimeZoneOffset, BamlClassTimeZonedDateTime,
    BamlNamespaceTime, PackageBamlImpl, copy, view,
};
use crate::{BexVm, errors::VmRustFnError};

use bex_vm_types::calendar as shared;
pub(crate) use bex_vm_types::calendar::{
    DATE_FORMAT, DATETIME_FORMAT, NANOS_PER_DAY, NANOS_PER_HOUR, NANOS_PER_MINUTE,
    NANOS_PER_SECOND, civil_datetime, date_for_days, date_from_components, days_since_epoch,
    format_clock_into, format_date_into,
};
impl BamlClassTimeInstant for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let value = shared::Instant::parse(s.as_str())?;
        let value = copy::time::Instant {
            _nanoseconds: Arc::new(value._nanoseconds),
        };
        Ok(value.to_value(vm))
    }
    fn _to_string_impl(
        instant: &view::time::Instant<'_>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        Ok(bex_str::BexStr::from(shared::Instant::_to_string_impl(
            &shared::Instant {
                _nanoseconds: instant._nanoseconds().as_ref().clone(),
            },
        )?))
    }
}
impl BamlClassTimePlainDate for PackageBamlImpl {
    fn from_components(
        vm: &mut BexVm,
        year: i64,
        month: i64,
        day: i64,
    ) -> Result<Value, VmRustFnError> {
        let value = shared::PlainDate::from_components(year, month, day)?;
        let value = copy::time::PlainDate { _days: value._days };
        Ok(value.to_value(vm))
    }
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let value = shared::PlainDate::parse(s.as_str())?;
        let value = copy::time::PlainDate { _days: value._days };
        Ok(value.to_value(vm))
    }
    fn _to_string_impl(
        plaindate: &view::time::PlainDate<'_>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        Ok(bex_str::BexStr::from(shared::PlainDate::_to_string_impl(
            &shared::PlainDate {
                _days: plaindate._days(),
            },
        )?))
    }
    fn _to_plain_datetime(vm: &mut BexVm, plaindate: &Value, time_ns: i64) -> Value {
        let value = shared::PlainDate::_to_plain_datetime(
            &shared::PlainDate {
                _days: view::time::PlainDate {
                    instance: vm.as_instance(plaindate).expect("time class"),
                }
                ._days(),
            },
            time_ns,
        );
        let value = copy::time::PlainDateTime {
            _nanoseconds: Arc::new(value._nanoseconds),
        };
        value.to_value(vm)
    }
    fn year(plaindate: &view::time::PlainDate<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDate::year(&shared::PlainDate {
            _days: plaindate._days(),
        })?)
    }
    fn month(plaindate: &view::time::PlainDate<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDate::month(&shared::PlainDate {
            _days: plaindate._days(),
        })?)
    }
    fn day(plaindate: &view::time::PlainDate<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDate::day(&shared::PlainDate {
            _days: plaindate._days(),
        })?)
    }
}
impl BamlClassTimePlainDateTime for PackageBamlImpl {
    fn _from_components(
        vm: &mut BexVm,
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<Value, VmRustFnError> {
        let value = shared::PlainDateTime::_from_components(
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
            microsecond,
            nanosecond,
        )?;
        let value = copy::time::PlainDateTime {
            _nanoseconds: Arc::new(value._nanoseconds),
        };
        Ok(value.to_value(vm))
    }
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let value = shared::PlainDateTime::parse(s.as_str())?;
        let value = copy::time::PlainDateTime {
            _nanoseconds: Arc::new(value._nanoseconds),
        };
        Ok(value.to_value(vm))
    }
    fn _to_string_impl(
        plaindatetime: &view::time::PlainDateTime<'_>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        Ok(bex_str::BexStr::from(
            shared::PlainDateTime::_to_string_impl(&shared::PlainDateTime {
                _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
            })?,
        ))
    }
    fn to_plain_date(vm: &mut BexVm, plaindatetime: &Value) -> Result<Value, VmRustFnError> {
        let value = shared::PlainDateTime::to_plain_date(&shared::PlainDateTime {
            _nanoseconds: view::time::PlainDateTime {
                instance: vm.as_instance(plaindatetime).expect("time class"),
            }
            ._nanoseconds()
            .as_ref()
            .clone(),
        })?;
        let value = copy::time::PlainDate { _days: value._days };
        Ok(value.to_value(vm))
    }
    fn to_plain_time(vm: &mut BexVm, plaindatetime: &Value) -> Value {
        let value = shared::PlainDateTime::to_plain_time(&shared::PlainDateTime {
            _nanoseconds: view::time::PlainDateTime {
                instance: vm.as_instance(plaindatetime).expect("time class"),
            }
            ._nanoseconds()
            .as_ref()
            .clone(),
        });
        let value = copy::time::PlainTime {
            _nanoseconds: value._nanoseconds,
        };
        value.to_value(vm)
    }
    fn year(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::year(&shared::PlainDateTime {
            _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
        })?)
    }
    fn month(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::month(&shared::PlainDateTime {
            _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
        })?)
    }
    fn day(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::day(&shared::PlainDateTime {
            _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
        })?)
    }
    fn hour(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::hour(&shared::PlainDateTime {
            _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
        })?)
    }
    fn minute(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::minute(&shared::PlainDateTime {
            _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
        })?)
    }
    fn second(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::second(&shared::PlainDateTime {
            _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
        })?)
    }
    fn millisecond(plaindatetime: &view::time::PlainDateTime<'_>) -> Result<i64, VmRustFnError> {
        Ok(shared::PlainDateTime::millisecond(
            &shared::PlainDateTime {
                _nanoseconds: plaindatetime._nanoseconds().as_ref().clone(),
            },
        )?)
    }
}
impl BamlClassTimePlainTime for PackageBamlImpl {
    fn _from_components(
        vm: &mut BexVm,
        hour: i64,
        minute: i64,
        second: i64,
        millisecond: i64,
        microsecond: i64,
        nanosecond: i64,
    ) -> Result<Value, VmRustFnError> {
        let value = shared::PlainTime::_from_components(
            hour,
            minute,
            second,
            millisecond,
            microsecond,
            nanosecond,
        )?;
        let value = copy::time::PlainTime {
            _nanoseconds: value._nanoseconds,
        };
        Ok(value.to_value(vm))
    }
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let value = shared::PlainTime::parse(s.as_str())?;
        let value = copy::time::PlainTime {
            _nanoseconds: value._nanoseconds,
        };
        Ok(value.to_value(vm))
    }
    fn _to_string_impl(plaintime: &view::time::PlainTime<'_>) -> bex_str::BexStr {
        bex_str::BexStr::from(shared::PlainTime::_to_string_impl(&shared::PlainTime {
            _nanoseconds: plaintime._nanoseconds(),
        }))
    }
}
impl BamlClassTimeTimeZoneOffset for PackageBamlImpl {
    fn from_duration(vm: &mut BexVm, duration: &Value) -> Result<Value, VmRustFnError> {
        let value = shared::TimeZoneOffset::from_duration(&shared::Duration {
            _nanoseconds: view::time::Duration {
                instance: vm.as_instance(duration).expect("time class"),
            }
            ._nanoseconds()
            .as_ref()
            .clone(),
        })?;
        let value = copy::time::TimeZoneOffset {
            _nanoseconds: value._nanoseconds,
        };
        Ok(value.to_value(vm))
    }
}
impl BamlClassTimeZonedDateTime for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let value = shared::ZonedDateTime::parse(s.as_str())?;
        let value = copy::time::ZonedDateTime {
            _nanoseconds: Arc::new(value._nanoseconds),
            _offset_ns: value
                ._offset_ns
                .map_or(Value::NULL, |n| Value::try_int(n).expect("time offset")),
            _iana: value
                ._iana
                .map_or(Value::NULL, |s| Value::object(vm.alloc_string(s))),
        };
        Ok(value.to_value(vm))
    }
}
impl BamlNamespaceTime for PackageBamlImpl {
    fn _format_zoned(
        epoch_ns: Arc<num_bigint::BigInt>,
        offset_ns: i64,
        iana: Option<&bex_str::BexStr>,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        Ok(bex_str::BexStr::from(shared::_format_zoned(
            (*epoch_ns).clone(),
            offset_ns,
            iana.map(|s| s.as_str()),
        )?))
    }
}
