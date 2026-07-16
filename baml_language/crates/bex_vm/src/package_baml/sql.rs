use std::sync::Arc;

use baml_builtins2::{SqlArrayType, SqlBindValue, SqlStatement};
use baml_type::RealizedTy;
use bex_heap::TlabHolder;
use bex_vm_types::{Object, Value, ValueKind};

use super::{BamlNamespaceSql, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

impl BamlNamespaceSql for PackageBamlImpl {
    fn _statement(
        vm: &mut BexVm,
        parts: &[Value],
        values: &[Value],
    ) -> Result<Value, VmRustFnError> {
        if parts.len() != values.len() + 1 {
            return Err(sql_error(
                vm,
                "Unsupported",
                "invalid tagged SQL statement: parts length must equal values length plus one",
            ));
        }

        let parts = parts
            .iter()
            .map(|part| {
                vm.as_string(part)
                    .map(|value| value.as_str().to_owned())
                    .map_err(VmRustFnError::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let values = values
            .iter()
            .map(|value| snapshot_bind(vm, *value))
            .collect::<Result<Vec<_>, _>>()?;
        let statement = Arc::new(SqlStatement { parts, values });
        let handle = Value::object(vm.alloc_rust_data(statement));
        let class = vm.resolve_class("baml.sql.Statement");
        Ok(Value::object(vm.alloc_instance(class, vec![handle])))
    }
}

fn snapshot_bind(vm: &mut BexVm, value: Value) -> Result<SqlBindValue, VmRustFnError> {
    match value.kind() {
        ValueKind::Null => Ok(SqlBindValue::Null),
        ValueKind::Bool(value) => Ok(SqlBindValue::Bool(value)),
        ValueKind::Int(value) => Ok(SqlBindValue::Int(value)),
        ValueKind::OmittedArg => Err(unsupported(vm, "omitted arguments cannot be SQL binds")),
        ValueKind::Object(pointer) => match vm.get_object(pointer) {
            Object::String(value) => Ok(SqlBindValue::String(value.as_str().to_owned())),
            Object::Bigint(value) => Ok(SqlBindValue::BigInt((**value).clone())),
            Object::Float(value) => Ok(SqlBindValue::Float(*value)),
            Object::Uint8Array(value) => Ok(SqlBindValue::Bytes(value.to_vec())),
            Object::Array(values) => {
                let element_type = sql_array_type(&values.element_ty);
                let source_values = values.to_vec();
                let Some(element_type) = element_type else {
                    return Err(unsupported(vm, "unsupported PostgreSQL array element type"));
                };
                let values = source_values
                    .iter()
                    .map(|value| snapshot_bind(vm, *value))
                    .collect::<Result<Vec<_>, _>>()?;
                if values
                    .iter()
                    .any(|value| matches!(value, SqlBindValue::Array { .. }))
                {
                    return Err(unsupported(vm, "nested arrays are not supported SQL binds"));
                }
                if values
                    .iter()
                    .any(|value| !array_value_matches(element_type, value))
                {
                    return Err(unsupported(vm, "SQL array binds must be homogeneous"));
                }
                Ok(SqlBindValue::Array {
                    element_type,
                    values,
                })
            }
            Object::Instance(instance) => {
                let class_name = match vm.get_object(instance.class) {
                    Object::Class(class) => class.name.render_dotted(false),
                    _ => return Err(unsupported(vm, "invalid class instance used as a SQL bind")),
                };
                let fields = (0..instance.field_len())
                    .map(|index| instance.load_field(index))
                    .collect::<Vec<_>>();
                snapshot_instance(vm, &class_name, &fields)
            }
            _ => Err(unsupported(
                vm,
                "value type is not supported as a SQL bind; convert it explicitly",
            )),
        },
    }
}

fn sql_array_type(ty: &RealizedTy) -> Option<SqlArrayType> {
    match ty {
        RealizedTy::Bool { .. } => Some(SqlArrayType::Bool),
        RealizedTy::Int { .. } => Some(SqlArrayType::Int),
        RealizedTy::Bigint { .. } => Some(SqlArrayType::BigInt),
        RealizedTy::Float { .. } => Some(SqlArrayType::Float),
        RealizedTy::String { .. } => Some(SqlArrayType::String),
        RealizedTy::Uint8Array { .. } => Some(SqlArrayType::Bytes),
        RealizedTy::Class(name, _, _) => match name.render_dotted(false).as_str() {
            "baml.sql.JsonValue" => Some(SqlArrayType::Json),
            "baml.time.Instant" => Some(SqlArrayType::Instant),
            "baml.time.ZonedDateTime" => Some(SqlArrayType::ZonedDateTime),
            "baml.time.PlainDateTime" => Some(SqlArrayType::PlainDateTime),
            "baml.time.PlainDate" => Some(SqlArrayType::PlainDate),
            "baml.time.PlainTime" => Some(SqlArrayType::PlainTime),
            "baml.time.Duration" => Some(SqlArrayType::Duration),
            _ => None,
        },
        RealizedTy::Union(members, _) => {
            let mut non_null = members
                .iter()
                .filter(|member| !matches!(member, RealizedTy::Null { .. }));
            let member = non_null.next()?;
            if non_null.next().is_some() {
                None
            } else {
                sql_array_type(member)
            }
        }
        _ => None,
    }
}

fn array_value_matches(element_type: SqlArrayType, value: &SqlBindValue) -> bool {
    matches!(value, SqlBindValue::Null)
        || matches!(
            (element_type, value),
            (SqlArrayType::Bool, SqlBindValue::Bool(_))
                | (SqlArrayType::Int, SqlBindValue::Int(_))
                | (SqlArrayType::BigInt, SqlBindValue::BigInt(_))
                | (SqlArrayType::Float, SqlBindValue::Float(_))
                | (SqlArrayType::String, SqlBindValue::String(_))
                | (SqlArrayType::Bytes, SqlBindValue::Bytes(_))
                | (SqlArrayType::Json, SqlBindValue::Json(_))
                | (SqlArrayType::Instant, SqlBindValue::Instant(_))
                | (
                    SqlArrayType::ZonedDateTime,
                    SqlBindValue::ZonedDateTime { .. }
                )
                | (SqlArrayType::PlainDateTime, SqlBindValue::PlainDateTime(_))
                | (SqlArrayType::PlainDate, SqlBindValue::PlainDate(_))
                | (SqlArrayType::PlainTime, SqlBindValue::PlainTime(_))
                | (SqlArrayType::Duration, SqlBindValue::Duration(_))
        )
}

fn snapshot_instance(
    vm: &mut BexVm,
    class_name: &str,
    fields: &[Value],
) -> Result<SqlBindValue, VmRustFnError> {
    match class_name {
        "baml.sql.JsonValue" => Ok(SqlBindValue::Json(super::json::value_to_serde(
            vm, fields[0],
        ))),
        "baml.time.Instant" => Ok(SqlBindValue::Instant(field_bigint(vm, fields, 0)?)),
        "baml.time.PlainDateTime" => Ok(SqlBindValue::PlainDateTime(field_bigint(vm, fields, 0)?)),
        "baml.time.PlainDate" => Ok(SqlBindValue::PlainDate(field_int(vm, fields, 0)?)),
        "baml.time.PlainTime" => Ok(SqlBindValue::PlainTime(field_int(vm, fields, 0)?)),
        "baml.time.Duration" => Ok(SqlBindValue::Duration(field_bigint(vm, fields, 0)?)),
        "baml.time.ZonedDateTime" => {
            let offset = if fields[1].is_null() {
                None
            } else {
                Some(field_int(vm, fields, 1)?)
            };
            let iana = if fields[2].is_null() {
                None
            } else {
                Some(
                    vm.as_string(&fields[2])
                        .map_err(VmRustFnError::from)?
                        .as_str()
                        .to_owned(),
                )
            };
            Ok(SqlBindValue::ZonedDateTime {
                epoch_nanoseconds: field_bigint(vm, fields, 0)?,
                offset_nanoseconds: offset,
                iana,
            })
        }
        _ => Err(unsupported(
            vm,
            "classes are not supported as SQL binds; wrap JSON explicitly with baml.sql.json",
        )),
    }
}

fn field_bigint(
    vm: &mut BexVm,
    fields: &[Value],
    index: usize,
) -> Result<num_bigint::BigInt, VmRustFnError> {
    vm.as_bigint(&fields[index])
        .map(|value| (**value).clone())
        .map_err(VmRustFnError::from)
}

fn field_int(vm: &mut BexVm, fields: &[Value], index: usize) -> Result<i64, VmRustFnError> {
    fields[index]
        .as_int()
        .ok_or_else(|| unsupported(vm, "invalid SQL time value"))
}

fn unsupported(vm: &mut BexVm, message: &str) -> VmRustFnError {
    sql_error(vm, "Unsupported", message)
}

fn sql_error(vm: &mut BexVm, kind: &str, message: &str) -> VmRustFnError {
    let class = vm.resolve_class("baml.sql.SqlError");
    let enum_ptr = vm.resolve_class("baml.sql.SqlErrorKind");
    let variant_index = match vm.get_object(enum_ptr) {
        Object::Enum(value) => value
            .variants
            .iter()
            .position(|variant| variant.name.as_str() == kind)
            .expect("SqlErrorKind variant must exist"),
        _ => {
            return VmInternalError::MissingNativeFunction {
                name: "baml.sql.SqlErrorKind".to_owned(),
            }
            .into();
        }
    };
    let kind = Value::object(vm.alloc_variant(enum_ptr, variant_index));
    let message = Value::object(vm.alloc_string(message.to_owned()));
    let error = Value::object(vm.alloc_instance(
        class,
        vec![
            kind,
            message,
            Value::NULL,
            Value::NULL,
            Value::NULL,
            Value::NULL,
        ],
    ));
    VmRustFnError::Thrown(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_variants_are_distinct_for_homogeneity() {
        assert!(array_value_matches(
            SqlArrayType::Int,
            &SqlBindValue::Int(1)
        ));
        assert!(!array_value_matches(
            SqlArrayType::Int,
            &SqlBindValue::BigInt(1.into())
        ));
    }
}
