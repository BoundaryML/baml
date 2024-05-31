use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    scan_args::get_kwargs, value::Value, Error, Float, Integer, RArray, RHash, RString, Ruby,
    Symbol,
};
use std::result::Result;

struct SerializationError {
    position: Vec<String>,
    message: String,
}

pub struct RubyToJson<'rb> {
    ruby: &'rb Ruby,
}

impl<'rb> RubyToJson<'rb> {
    pub fn roundtrip(from: Value) -> crate::Result<Value> {
        let json = RubyToJson::convert(from)?;
        serde_magnus::serialize(&json)
    }

    /// Convert a Ruby object to a JSON object.
    ///
    /// We have to implement this ourselves instead of relying on Serde, because in the codegen,
    /// we can't convert a BAML-generated type to a hash trivially (specifically union-typed
    /// fields do not serialize correctly, see https://sorbet.org/docs/tstruct#serialize-gotchas)
    ///
    /// We do still rely on :serialize for enums though.
    pub fn convert(from: Value) -> crate::Result<serde_json::Value> {
        let ruby = Ruby::get_with(from);
        let result = RubyToJson { ruby: &ruby }.to_json(from, vec![]);

        match result {
            Ok(value) => Ok(value),
            Err(e) => {
                let mut errors = vec![];
                for error in e {
                    errors.push(format!("  {}: {}", error.position.join("."), error.message));
                }
                Err(Error::new(
                    ruby.exception_type_error(),
                    format!(
                        "failed to convert Ruby object to JSON, errors were:\n{}\nRuby object:\n{}",
                        errors.join("\n"),
                        from.inspect()
                    ),
                ))
            }
        }
    }

    pub fn convert_hash_to_json(
        from: RHash,
    ) -> crate::Result<serde_json::Map<String, serde_json::Value>> {
        let ruby = Ruby::get_with(from);
        let result = RubyToJson { ruby: &ruby }.hash_to_map(from, vec![]);

        match result {
            Ok(value) => Ok(value),
            Err(e) => {
                let mut errors = vec![];
                for error in e {
                    errors.push(format!("  {}: {}", error.position.join("."), error.message));
                }
                Err(Error::new(
                    ruby.exception_type_error(),
                    format!(
                        "failed to convert Ruby object to JSON, errors were:\n{}\nRuby object:\n{}",
                        errors.join("\n"),
                        from.inspect()
                    ),
                ))
            }
        }
    }

    fn to_json(
        &self,
        any: Value,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        // done - self.ruby.class_float();
        // done - self.ruby.class_integer();
        // done - self.ruby.class_true_class();
        // done - self.ruby.class_false_class();
        // done - self.ruby.class_nil_class();
        // done - self.ruby.class_array();
        // done - self.ruby.class_string();
        // done? - self.ruby.class_hash();
        // enums
        // done - structs

        if any.is_nil() {
            return Ok(serde_json::Value::Null);
        }

        if any.is_kind_of(self.ruby.class_true_class()) {
            return Ok(serde_json::Value::Bool(true));
        }

        if any.is_kind_of(self.ruby.class_false_class()) {
            return Ok(serde_json::Value::Bool(false));
        }

        if let Some(any) = magnus::Integer::from_value(any) {
            return self.to_int(any, field_pos);
        }

        if let Some(any) = magnus::Float::from_value(any) {
            return self.to_float(any, field_pos);
        }

        if let Some(any) = RString::from_value(any) {
            return self
                .to_string(any, field_pos)
                .map(serde_json::Value::String);
        }

        if let Some(any) = RArray::from_value(any) {
            return self.to_array(any, field_pos);
        }

        if let Some(any) = RHash::from_value(any) {
            return self
                .hash_to_map(any, field_pos)
                .map(serde_json::Value::Object);
        }

        if let Ok(superclass) = any.class().superclass() {
            let superclass = unsafe { superclass.name() }.to_owned().to_string();

            if superclass == "T::Struct" {
                return self.struct_to_map(any, field_pos);
            }

            if superclass == "T::Enum" {
                return self.sorbet_to_json(any, field_pos);
            }
        }

        Err(vec![SerializationError {
            position: field_pos,
            message: format!(
                "JSON conversion not supported for value of type {:?}",
                any.class()
            ),
        }])
    }

    fn to_int(
        &self,
        any: Integer,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        if let Ok(any) = any.to_i64() {
            return Ok(serde_json::Value::Number(serde_json::Number::from(any)));
        }
        if let Ok(any) = any.to_u64() {
            return Ok(serde_json::Value::Number(serde_json::Number::from(any)));
        }

        return Err(vec![SerializationError {
            position: field_pos,
            message: format!("failed to convert {:?} to i64 or u64", any),
        }]);
    }

    fn to_float(
        &self,
        any: Float,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        let Some(as_json) = serde_json::Number::from_f64(any.to_f64()) else {
            return Err(vec![SerializationError {
                position: field_pos,
                message: format!("failed to convert {:?} to float", any),
            }]);
        };
        return Ok(serde_json::Value::Number(as_json));
    }

    fn to_string(
        &self,
        any: RString,
        field_pos: Vec<String>,
    ) -> Result<String, Vec<SerializationError>> {
        let Ok(any) = any.to_string() else {
            return Err(vec![SerializationError {
                position: field_pos,
                message: format!("cannot convert {:#?} to utf-8 string", any),
            }]);
        };
        return Ok(any);
    }

    fn to_array(
        &self,
        any: RArray,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        let mut errs = vec![];
        let mut arr = vec![];

        for (i, value) in any.each().enumerate() {
            let mut field_pos = field_pos.clone();
            field_pos.push(i.to_string());

            let Ok(value) = value else {
                errs.push(SerializationError {
                    position: field_pos.clone(),
                    message: format!("failed to enumerate array element at index {}", i),
                });
                continue;
            };
            match self.to_json(value, field_pos) {
                Ok(json_value) => {
                    arr.push(json_value);
                }
                Err(e) => errs.extend(e),
            }
        }

        if !errs.is_empty() {
            return Err(errs);
        }

        return Ok(serde_json::Value::Array(arr));
    }

    fn hash_key_to_string(
        &self,
        k: Value,
        field_pos: Vec<String>,
    ) -> Result<String, Vec<SerializationError>> {
        if let Some(k) = Symbol::from_value(k) {
            return match k.name() {
                Ok(k) => Ok(k.to_string()),
                Err(_) => Err(vec![SerializationError {
                    position: field_pos.clone(),
                    message: format!("failed to convert symbol key in hash to string: {:#?}", k),
                }]),
            };
        }
        if let Some(k) = RString::from_value(k) {
            let mut field_pos = field_pos.clone();
            field_pos.push(format!("{:#?}", k));
            return match self.to_string(k, field_pos) {
                Ok(k) => Ok(k),
                Err(errs) => Err(errs
                    .into_iter()
                    .map(|mut e| {
                        e.message =
                            format!("failed to convert string key in hash to string: {:#?}", k);
                        e
                    })
                    .collect()),
            };
        }
        Err(vec![SerializationError {
            position: field_pos.clone(),
            message: format!(
                "expected every key in this hash to be symbol or string, but found key {:#?}",
                k
            ),
        }])
    }

    fn hash_to_map(
        &self,
        any: RHash,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Map<String, serde_json::Value>, Vec<SerializationError>> {
        use magnus::r_hash::ForEach;

        let mut errs = vec![];
        let mut map = serde_json::Map::new();
        if any
            .foreach(|k: Value, v: Value| {
                let k = match self.hash_key_to_string(k, field_pos.clone()) {
                    Ok(k) => k,
                    Err(e) => {
                        errs.extend(e);
                        return Ok(ForEach::Continue);
                    }
                };

                let mut field_pos = field_pos.clone();
                field_pos.push(k.clone());

                match self.to_json(v, field_pos.clone()) {
                    Ok(json_value) => {
                        map.insert(k.to_string(), json_value);
                    }
                    Err(e) => errs.extend(e),
                }
                Ok(ForEach::Continue)
            })
            .is_err()
        {
            errs.push(SerializationError {
                position: field_pos.clone(),
                message: "failed to iterate over hash".to_string(),
            });
        };

        if !errs.is_empty() {
            return Err(errs);
        }

        return Ok(map);
    }

    fn struct_to_map(
        &self,
        any: Value,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        // https://ruby-doc.org/3.0.4/Module.html#method-i-instance_methods
        let fields = match any
            .class()
            .check_funcall::<_, _, Value>("instance_methods", (self.ruby.qfalse(),))
        {
            None => {
                return Err(vec![SerializationError {
                    position: field_pos,
                    message: format!("class does not respond to :instance_methods"),
                }]);
            }
            Some(Err(e)) => {
                return Err(vec![SerializationError {
                    position: field_pos,
                    message: format!(".class.instance_methods returned error: {e}"),
                }]);
            }
            Some(Ok(fields)) => match RArray::from_value(fields) {
                None => {
                    return Err(vec![SerializationError {
                        position: field_pos,
                        message: format!(".class.instance_methods was non-array: {fields:?}"),
                    }]);
                }
                Some(fields) => {
                    let fields = fields.each().collect::<crate::Result<Vec<_>>>();
                    let fields = match fields {
                        Err(e) => {
                            return Err(vec![SerializationError {
                                position: field_pos,
                                message: format!(".class.instance_methods.each failed: {e:?}"),
                            }]);
                        }
                        Ok(fields) => fields,
                    };
                    let fields = fields
                        .into_iter()
                        .map(|v| {
                            Symbol::from_value(v)
                                .ok_or(format!("failed to convert {:#?} to symbol", v))
                        })
                        .collect::<Result<Vec<_>, std::string::String>>();
                    match fields {
                        Err(e) => {
                            return Err(vec![SerializationError {
                                position: field_pos,
                                message: format!(
                                    "failed to convert .class.instance_methods to array of symbols: {e}"
                                ),
                            }]);
                        }
                        Ok(fields) => fields
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<String>>(),
                    }
                }
            },
        };

        let mut errs = vec![];
        let mut map = serde_json::Map::new();
        for field in fields.as_slice() {
            let mut field_pos = field_pos.clone();
            field_pos.push(field.clone());

            let value = match any.funcall(field.clone(), ()) {
                Ok(value) => value,
                Err(e) => {
                    return Err(vec![SerializationError {
                        position: field_pos,
                        message: format!("object responded to :{field} with error: {e}"),
                    }]);
                }
            };

            match self.to_json(value, field_pos) {
                Ok(json_value) => {
                    map.insert(field.clone(), json_value);
                }
                Err(e) => {
                    errs.extend(e);
                }
            };
        }

        if !errs.is_empty() {
            return Err(errs);
        }

        Ok(serde_json::Value::Object(map))
    }

    // This codepath is not used right now - it was implemented as a proof-of-concept
    // for serializing structs to JSON, by combining :to_hash with this method. If the
    // implementation of struct_to_map proves to be stable, we can get rid of this.
    #[allow(dead_code)]
    fn struct_to_map_via_to_hash(
        &self,
        any: Result<Value, Error>,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        let any = match any {
            Ok(any) => any,
            Err(e) => {
                return Err(vec![SerializationError {
                    position: field_pos,
                    message: format!("struct responded to :to_hash with error: {e}"),
                }])
            }
        };

        if let Some(any) = RHash::from_value(any) {
            return self
                .hash_to_map(any, field_pos)
                .map(serde_json::Value::Object);
        }

        return Err(vec![SerializationError {
            position: field_pos,
            message: format!(
                "struct did not respond to :to_hash with a hash, was: {:#?}",
                any
            ),
        }]);
    }

    fn sorbet_to_json(
        &self,
        any: Value,
        field_pos: Vec<String>,
    ) -> Result<serde_json::Value, Vec<SerializationError>> {
        match any.check_funcall("serialize", ()) {
            None => {
                return Err(vec![SerializationError {
                    position: field_pos,
                    message: format!("object does not respond to :serialize"),
                }]);
            }
            Some(Err(e)) => {
                return Err(vec![SerializationError {
                    position: field_pos,
                    message: format!("object responded to :serialize with error: {e}"),
                }]);
            }
            Some(Ok(any)) => {
                return self.to_json(any, field_pos);
            }
        };
    }
}
