//! Owned calendar boundary around the shared heap-neutral algorithms.
use crate::{Error, R};
use bex_external_types::BexExternalValue as V;
use bex_vm_types::calendar as shared;
fn invalid() -> Box<Error> {
    Error::internal("invalid calendar boundary value".into())
}
fn text(v: &V) -> R<&str> {
    if let V::String(s) = v {
        Ok(s.as_str())
    } else {
        Err(invalid())
    }
}
fn int(v: &V) -> R<i64> {
    if let V::Int(n) = v {
        Ok(*n)
    } else {
        Err(invalid())
    }
}
fn bigint(v: &V) -> R<num_bigint::BigInt> {
    if let V::Bigint(n) = v {
        Ok(n.clone())
    } else {
        Err(invalid())
    }
}
macro_rules! rt_fields {($($key:literal=>$value:expr),* $(,)?)=>{indexmap::indexmap!{$($key.into()=>$value),*}};}
trait Class: Sized {
    fn read(v: &V) -> R<Self>;
    fn owned(self) -> V;
}
impl Class for shared::Instant {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _nanoseconds: bigint(fields.get("_nanoseconds").ok_or_else(invalid)?)?,
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.Instant".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_nanoseconds"=>V::Bigint(self._nanoseconds)},
        }
    }
}
impl Class for shared::PlainDate {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _days: int(fields.get("_days").ok_or_else(invalid)?)?,
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.PlainDate".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_days"=>V::Int(self._days)},
        }
    }
}
impl Class for shared::PlainDateTime {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _nanoseconds: bigint(fields.get("_nanoseconds").ok_or_else(invalid)?)?,
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.PlainDateTime".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_nanoseconds"=>V::Bigint(self._nanoseconds)},
        }
    }
}
impl Class for shared::PlainTime {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _nanoseconds: int(fields.get("_nanoseconds").ok_or_else(invalid)?)?,
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.PlainTime".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_nanoseconds"=>V::Int(self._nanoseconds)},
        }
    }
}
impl Class for shared::TimeZoneOffset {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _nanoseconds: int(fields.get("_nanoseconds").ok_or_else(invalid)?)?,
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.TimeZoneOffset".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_nanoseconds"=>V::Int(self._nanoseconds)},
        }
    }
}
impl Class for shared::ZonedDateTime {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _nanoseconds: bigint(fields.get("_nanoseconds").ok_or_else(invalid)?)?,
            _offset_ns: if matches!(fields.get("_offset_ns").ok_or_else(invalid)?, V::Null) {
                None
            } else {
                Some(int(fields.get("_offset_ns").ok_or_else(invalid)?)?)
            },
            _iana: if matches!(fields.get("_iana").ok_or_else(invalid)?, V::Null) {
                None
            } else {
                Some(text(fields.get("_iana").ok_or_else(invalid)?)?.to_owned())
            },
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.ZonedDateTime".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_nanoseconds"=>V::Bigint(self._nanoseconds),"_offset_ns"=>self._offset_ns.map_or(V::Null,V::Int),"_iana"=>self._iana.map_or(V::Null,|s|V::String(s.into()))},
        }
    }
}
impl Class for shared::Duration {
    fn read(v: &V) -> R<Self> {
        let V::Instance { fields, .. } = v else {
            return Err(invalid());
        };
        Ok(Self {
            _nanoseconds: bigint(fields.get("_nanoseconds").ok_or_else(invalid)?)?,
        })
    }
    fn owned(self) -> V {
        V::Instance {
            class_name: "baml.time.Duration".into(),
            type_args: Vec::new(),
            fields: rt_fields! {"_nanoseconds"=>V::Bigint(self._nanoseconds)},
        }
    }
}
pub fn call(path: &str, args: &[V]) -> R<V> {
    Ok(match path {
        "baml.time.Instant.parse" => {
            shared::Instant::parse(text(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?
                .owned()
        }
        "baml.time.Instant._to_string_impl" => V::String(
            (shared::Instant::_to_string_impl(&shared::Instant::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?)
            .into(),
        ),
        "baml.time.PlainDate.from_components" => shared::PlainDate::from_components(
            int(args.get(0).ok_or_else(invalid)?)?,
            int(args.get(1).ok_or_else(invalid)?)?,
            int(args.get(2).ok_or_else(invalid)?)?,
        )
        .map_err(crate::scalars::error)?
        .owned(),
        "baml.time.PlainDate.parse" => {
            shared::PlainDate::parse(text(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?
                .owned()
        }
        "baml.time.PlainDate._to_string_impl" => V::String(
            (shared::PlainDate::_to_string_impl(&shared::PlainDate::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?)
            .into(),
        ),
        "baml.time.PlainDate._to_plain_datetime" => shared::PlainDate::_to_plain_datetime(
            &shared::PlainDate::read(args.get(0).ok_or_else(invalid)?)?,
            int(args.get(1).ok_or_else(invalid)?)?,
        )
        .owned(),
        "baml.time.PlainDate.year" => V::Int(
            shared::PlainDate::year(&shared::PlainDate::read(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDate.month" => V::Int(
            shared::PlainDate::month(&shared::PlainDate::read(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDate.day" => V::Int(
            shared::PlainDate::day(&shared::PlainDate::read(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime._from_components" => shared::PlainDateTime::_from_components(
            int(args.get(0).ok_or_else(invalid)?)?,
            int(args.get(1).ok_or_else(invalid)?)?,
            int(args.get(2).ok_or_else(invalid)?)?,
            int(args.get(3).ok_or_else(invalid)?)?,
            int(args.get(4).ok_or_else(invalid)?)?,
            int(args.get(5).ok_or_else(invalid)?)?,
            int(args.get(6).ok_or_else(invalid)?)?,
            int(args.get(7).ok_or_else(invalid)?)?,
            int(args.get(8).ok_or_else(invalid)?)?,
        )
        .map_err(crate::scalars::error)?
        .owned(),
        "baml.time.PlainDateTime.parse" => {
            shared::PlainDateTime::parse(text(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?
                .owned()
        }
        "baml.time.PlainDateTime._to_string_impl" => V::String(
            (shared::PlainDateTime::_to_string_impl(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?)
            .into(),
        ),
        "baml.time.PlainDateTime.to_plain_date" => shared::PlainDateTime::to_plain_date(
            &shared::PlainDateTime::read(args.get(0).ok_or_else(invalid)?)?,
        )
        .map_err(crate::scalars::error)?
        .owned(),
        "baml.time.PlainDateTime.to_plain_time" => shared::PlainDateTime::to_plain_time(
            &shared::PlainDateTime::read(args.get(0).ok_or_else(invalid)?)?,
        )
        .owned(),
        "baml.time.PlainDateTime.year" => V::Int(
            shared::PlainDateTime::year(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime.month" => V::Int(
            shared::PlainDateTime::month(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime.day" => V::Int(
            shared::PlainDateTime::day(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime.hour" => V::Int(
            shared::PlainDateTime::hour(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime.minute" => V::Int(
            shared::PlainDateTime::minute(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime.second" => V::Int(
            shared::PlainDateTime::second(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainDateTime.millisecond" => V::Int(
            shared::PlainDateTime::millisecond(&shared::PlainDateTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?)
            .map_err(crate::scalars::error)?,
        ),
        "baml.time.PlainTime._from_components" => shared::PlainTime::_from_components(
            int(args.get(0).ok_or_else(invalid)?)?,
            int(args.get(1).ok_or_else(invalid)?)?,
            int(args.get(2).ok_or_else(invalid)?)?,
            int(args.get(3).ok_or_else(invalid)?)?,
            int(args.get(4).ok_or_else(invalid)?)?,
            int(args.get(5).ok_or_else(invalid)?)?,
        )
        .map_err(crate::scalars::error)?
        .owned(),
        "baml.time.PlainTime.parse" => {
            shared::PlainTime::parse(text(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?
                .owned()
        }
        "baml.time.PlainTime._to_string_impl" => V::String(
            (shared::PlainTime::_to_string_impl(&shared::PlainTime::read(
                args.get(0).ok_or_else(invalid)?,
            )?))
            .into(),
        ),
        "baml.time.TimeZoneOffset.from_duration" => shared::TimeZoneOffset::from_duration(
            &shared::Duration::read(args.get(0).ok_or_else(invalid)?)?,
        )
        .map_err(crate::scalars::error)?
        .owned(),
        "baml.time.ZonedDateTime.parse" => {
            shared::ZonedDateTime::parse(text(args.get(0).ok_or_else(invalid)?)?)
                .map_err(crate::scalars::error)?
                .owned()
        }
        "baml.time._format_zoned" => V::String(
            (shared::_format_zoned(
                bigint(args.get(0).ok_or_else(invalid)?)?,
                int(args.get(1).ok_or_else(invalid)?)?,
                match args.get(2).ok_or_else(invalid)? {
                    V::Null => None,
                    v => Some(text(v)?),
                },
            )
            .map_err(crate::scalars::error)?)
            .into(),
        ),
        _ => return Err(invalid()),
    })
}
