use std::sync::Arc;

use datafusion::arrow::array::{
    Array, BooleanBuilder, LargeBinaryArray, LargeBinaryBuilder, StringBuilder,
};
use datafusion::arrow::datatypes::DataType;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use datafusion::scalar::ScalarValue;
use serde_json::Value;

#[derive(Debug, Eq, Hash, PartialEq)]
struct ValueAt {
    signature: Signature,
}

impl ScalarUDFImpl for ValueAt {
    fn name(&self) -> &'static str {
        "value_at"
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _arg_types: &[DataType]) -> DataFusionResult<DataType> {
        Ok(DataType::LargeBinary)
    }
    fn invoke_with_args(
        &self,
        function_args: ScalarFunctionArgs,
    ) -> DataFusionResult<ColumnarValue> {
        let args = function_args.args;
        let values = match args.first() {
            Some(ColumnarValue::Array(array)) => array
                .as_any()
                .downcast_ref::<LargeBinaryArray>()
                .ok_or_else(|| {
                    DataFusionError::Execution("value_at expects BamlValue bytes".to_owned())
                })?,
            _ => {
                return Err(DataFusionError::Execution(
                    "value_at expects an array value".to_owned(),
                ));
            }
        };
        let index = match args.get(1) {
            Some(ColumnarValue::Scalar(ScalarValue::Int64(Some(index)))) if *index >= 0 => {
                usize::try_from(*index).map_err(|_| {
                    DataFusionError::Execution("value_at index is too large".to_owned())
                })?
            }
            Some(ColumnarValue::Scalar(ScalarValue::Int32(Some(index)))) if *index >= 0 => {
                usize::try_from(*index).map_err(|_| {
                    DataFusionError::Execution("value_at index is too large".to_owned())
                })?
            }
            _ => {
                return Err(DataFusionError::Execution(
                    "value_at expects a non-negative integer index".to_owned(),
                ));
            }
        };
        let mut output = LargeBinaryBuilder::new();
        for row in 0..values.len() {
            if values.is_null(row) {
                output.append_null();
                continue;
            }
            let value: Value = serde_json::from_slice(values.value(row))
                .map_err(|error| DataFusionError::Execution(error.to_string()))?;
            match value.get(index) {
                Some(value) => output.append_value(
                    serde_json::to_vec(value)
                        .map_err(|error| DataFusionError::Execution(error.to_string()))?,
                ),
                None => output.append_null(),
            }
        }
        Ok(ColumnarValue::Array(Arc::new(output.finish())))
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct Contains {
    signature: Signature,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct ValueField {
    signature: Signature,
}

impl ScalarUDFImpl for ValueField {
    fn name(&self) -> &'static str {
        "value_field"
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> DataFusionResult<DataType> {
        Ok(DataType::LargeBinary)
    }

    fn invoke_with_args(
        &self,
        function_args: ScalarFunctionArgs,
    ) -> DataFusionResult<ColumnarValue> {
        let values = binary_argument(&function_args.args, "value_field")?;
        let field = string_argument(&function_args.args, 1, "value_field")?;
        let mut output = LargeBinaryBuilder::new();
        for row in 0..values.len() {
            if values.is_null(row) {
                output.append_null();
                continue;
            }
            let value: Value = serde_json::from_slice(values.value(row))
                .map_err(|error| DataFusionError::Execution(error.to_string()))?;
            match value.get(field) {
                Some(value) => output.append_value(
                    serde_json::to_vec(value)
                        .map_err(|error| DataFusionError::Execution(error.to_string()))?,
                ),
                None => output.append_null(),
            }
        }
        Ok(ColumnarValue::Array(Arc::new(output.finish())))
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct ValueString {
    signature: Signature,
}

impl ScalarUDFImpl for ValueString {
    fn name(&self) -> &'static str {
        "value_string"
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> DataFusionResult<DataType> {
        Ok(DataType::Utf8)
    }

    fn invoke_with_args(
        &self,
        function_args: ScalarFunctionArgs,
    ) -> DataFusionResult<ColumnarValue> {
        let values = binary_argument(&function_args.args, "value_string")?;
        let mut output = StringBuilder::new();
        for row in 0..values.len() {
            if values.is_null(row) {
                output.append_null();
                continue;
            }
            let value: Value = serde_json::from_slice(values.value(row))
                .map_err(|error| DataFusionError::Execution(error.to_string()))?;
            match value.as_str() {
                Some(value) => output.append_value(value),
                None => output.append_null(),
            }
        }
        Ok(ColumnarValue::Array(Arc::new(output.finish())))
    }
}

impl ScalarUDFImpl for Contains {
    fn name(&self) -> &'static str {
        "contains"
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _arg_types: &[DataType]) -> DataFusionResult<DataType> {
        Ok(DataType::Boolean)
    }
    fn invoke_with_args(
        &self,
        function_args: ScalarFunctionArgs,
    ) -> DataFusionResult<ColumnarValue> {
        let args = function_args.args;
        let values = match args.first() {
            Some(ColumnarValue::Array(array)) => array
                .as_any()
                .downcast_ref::<LargeBinaryArray>()
                .ok_or_else(|| {
                    DataFusionError::Execution("contains expects BamlValue bytes".to_owned())
                })?,
            _ => {
                return Err(DataFusionError::Execution(
                    "contains expects an array value".to_owned(),
                ));
            }
        };
        let needle = match args.get(1) {
            Some(ColumnarValue::Scalar(ScalarValue::Utf8(Some(value)))) => value.as_str(),
            Some(ColumnarValue::Scalar(ScalarValue::LargeUtf8(Some(value)))) => value.as_str(),
            _ => {
                return Err(DataFusionError::Execution(
                    "contains expects a string literal".to_owned(),
                ));
            }
        };
        let mut output = BooleanBuilder::new();
        for row in 0..values.len() {
            if values.is_null(row) {
                output.append_null();
                continue;
            }
            let value: Value = serde_json::from_slice(values.value(row))
                .map_err(|error| DataFusionError::Execution(error.to_string()))?;
            match value.as_str() {
                Some(value) => output.append_value(value.contains(needle)),
                None => output.append_null(),
            }
        }
        Ok(ColumnarValue::Array(Arc::new(output.finish())))
    }
}

pub fn register_builtin_functions(ctx: &datafusion::execution::context::SessionContext) {
    ctx.register_udf(ScalarUDF::new_from_impl(ValueAt {
        signature: Signature::exact(
            vec![DataType::LargeBinary, DataType::Int64],
            Volatility::Immutable,
        ),
    }));
    ctx.register_udf(ScalarUDF::new_from_impl(Contains {
        signature: Signature::exact(
            vec![DataType::LargeBinary, DataType::Utf8],
            Volatility::Immutable,
        ),
    }));
    ctx.register_udf(ScalarUDF::new_from_impl(ValueField {
        signature: Signature::exact(
            vec![DataType::LargeBinary, DataType::Utf8],
            Volatility::Immutable,
        ),
    }));
    ctx.register_udf(ScalarUDF::new_from_impl(ValueString {
        signature: Signature::exact(vec![DataType::LargeBinary], Volatility::Immutable),
    }));
}

fn binary_argument<'a>(
    args: &'a [ColumnarValue],
    function_name: &str,
) -> DataFusionResult<&'a LargeBinaryArray> {
    match args.first() {
        Some(ColumnarValue::Array(array)) => array
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .ok_or_else(|| {
                DataFusionError::Execution(format!("{function_name} expects BamlValue bytes"))
            }),
        _ => Err(DataFusionError::Execution(format!(
            "{function_name} expects an array value"
        ))),
    }
}

fn string_argument<'a>(
    args: &'a [ColumnarValue],
    index: usize,
    function_name: &str,
) -> DataFusionResult<&'a str> {
    match args.get(index) {
        Some(ColumnarValue::Scalar(ScalarValue::Utf8(Some(value)))) => Ok(value),
        Some(ColumnarValue::Scalar(ScalarValue::LargeUtf8(Some(value)))) => Ok(value),
        _ => Err(DataFusionError::Execution(format!(
            "{function_name} expects a string literal"
        ))),
    }
}
