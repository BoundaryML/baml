use magnus::{
    class, define_class,
    encoding::{CType, RbEncoding},
    exception::type_error,
    function, method,
    prelude::*,
    scan_args::get_kwargs,
    value::Value,
    Attr, Error, IntoValue, KwArgs, RArray, RClass, RHash, RObject, RString, Ruby,
};

type Result<T> = std::result::Result<T, Error>;

pub struct JsonToRuby<'rb> {
    ruby: &'rb Ruby,
}

struct PretendStruct {}

impl JsonToRuby<'_> {
    pub fn to_ruby(ruby: &Ruby, json: serde_json::Value) -> Result<Value> {
        JsonToRuby { ruby: &ruby }.convert(json)
    }

    fn convert(&self, json: serde_json::Value) -> Result<Value> {
        match json {
            serde_json::Value::Null => Ok(self.ruby.qnil().into_value()),
            serde_json::Value::Bool(b) => Ok(if b {
                self.ruby.qtrue().into_value()
            } else {
                self.ruby.qfalse().into_value()
            }),
            serde_json::Value::Number(n) => {
                if let Some(n) = n.as_i64() {
                    return Ok(self.ruby.integer_from_i64(n).into_value());
                }
                if let Some(n) = n.as_u64() {
                    return Ok(self.ruby.integer_from_u64(n).into_value());
                }
                if let Some(n) = n.as_f64() {
                    match self.ruby.r_float_from_f64(n) {
                        Ok(f) => return Ok(f.into_value()),
                        Err(f) => return Ok(f.into_value()),
                    }
                }
                return Err(Error::new(
                    self.ruby.exception_type_error(),
                    format!("Failed to convert number to Ruby object: {}", n),
                ));
            }
            serde_json::Value::String(s) => Ok(self.ruby.str_new(s.as_str()).into_value()),
            serde_json::Value::Array(a) => {
                let mut arr = self.ruby.ary_new_capa(a.len());
                for v in a {
                    arr.push(self.convert(v)?)?;
                }
                Ok(arr.into_value())
            }
            serde_json::Value::Object(o) => {
                // We can't use define_struct - magnus caps it at 12 fields
                let class = RClass::new(self.ruby.class_object())?;
                for (k, _) in &o {
                    class.define_attr(k.as_str(), Attr::Read);
                }
                let Some(mut obj) = RObject::from_value(class.new_instance(())?) else {
                    return Err(Error::new(
                        self.ruby.exception_type_error(),
                        format!("Failed to convert map to Ruby object: {:?}", o),
                    ));
                };
                for (k, v) in o {
                    obj.ivar_set(format!("@{k}"), self.convert(v)?)?;
                }
                Ok(obj.into_value())
            }
        }
    }
}
