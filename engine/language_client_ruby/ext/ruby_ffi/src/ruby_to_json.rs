use baml_types::{BamlValue, BamlMap, BamlValueWithMeta, ResponseCheck};
use indexmap::IndexMap;
use magnus::{
    prelude::*, typed_data::Obj, value::Value, class, Error, Float, Integer, IntoValue, RArray, RClass,
    RHash, RModule, RString, Ruby, Symbol, TypedData,
};
use std::result::Result;

use crate::types::{
    self,
    media::{Audio, Image},
};

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

    pub fn type_name_for_checks(checks: &Vec<ResponseCheck>) -> String {
        let mut name = "Checks".to_string();
        let mut names: Vec<&String> = checks.iter().map(|ResponseCheck{name, ..}| name).collect();
        names.sort();
        for check_name in names.iter() {
            name.push_str("__");
            name.push_str(check_name);
        }
        name
    }

    /// Serialize a list of check results into some `Checked__*` instance.
    pub fn serialize_response_checks(ruby: &Ruby, checks: &Vec<ResponseCheck>) -> crate::Result<Value> {

        let class_name = format!("Types::{}", Self::type_name_for_checks(checks));
        let checks_class = ruby.eval::<RClass>(&class_name)?;

        // Create a `Check` for each check in the `Checked__*`.
        let hash = ruby.hash_new();
        checks.iter().try_for_each(|ResponseCheck{name, expression, status}| {
            let check_class = ruby.eval::<RClass>("Baml::Checks::Check")?;
            let check_hash = ruby.hash_new();
            check_hash.aset(ruby.sym_new("name"), name.as_str())?;
            check_hash.aset(ruby.sym_new("expr"), expression.as_str())?;
            check_hash.aset(ruby.sym_new("status"), status.as_str())?;

            let check: Value = check_class.funcall("new", (check_hash,))?;
            hash.aset(ruby.sym_new(name.as_str()), check)?;
            crate::Result::Ok(())
        })?;

        checks_class.funcall("new", (hash,))
    }

    pub fn serialize_baml(ruby: &Ruby, types: RModule, mut from: BamlValueWithMeta<Vec<ResponseCheck>>) -> crate::Result<Value> {

        // If we encounter a BamlValue node with check results, serialize it as
        // { value: T, checks: K }. To compute `value`, we strip the metadata
        // off the node and pass it back to `serialize_baml`.
        if !from.meta().is_empty() {
            let meta = from.meta().clone();
            let checks = Self::serialize_response_checks(ruby, &meta)?;

            *from.meta_mut() = vec![];
            let serialized_subvalue = Self::serialize_baml(ruby, types, from)?;

            let checked_class = ruby.eval::<RClass>("Baml::Checked")?;
            let hash = ruby.hash_new();
            hash.aset(ruby.sym_new("value"), serialized_subvalue)?;
            hash.aset(ruby.sym_new("checks"), checks)?;
            Ok(checked_class.funcall("new", (hash,))?)
        }
        // Otherwise encode it directly.
        else {
            match from {
                BamlValueWithMeta::Class(class_name, class_fields, _) => {
                    let hash = ruby.hash_new();
                    for (k, v) in class_fields.into_iter() {
                        let k = ruby.sym_new(k.as_str());
                        let v = RubyToJson::serialize_baml(ruby, types, v)?;
                        hash.aset(k, v)?;
                    }
                    match types.const_get::<_, RClass>(class_name.as_str()) {
                        Ok(class_type) => class_type.funcall("new", (hash,)),
                        Err(_) => {
                            let dynamic_class_type = ruby.eval::<RClass>("Baml::DynamicStruct")?;
                            dynamic_class_type.funcall("new", (hash,))
                        }
                    }
                }
                BamlValueWithMeta::Enum(enum_name, enum_value, _) => {
                    if let Ok(enum_type) = types.const_get::<_, RClass>(enum_name.as_str()) {
                        let enum_value = ruby.str_new(&enum_value);
                        if let Ok(enum_instance) = enum_type.funcall("deserialize", (enum_value,)) {
                            return Ok(enum_instance);
                        }
                    }

                    Ok(ruby.str_new(&enum_value).into_value_with(ruby))
                }
                BamlValueWithMeta::Map(m,_) => {
                    let hash = ruby.hash_new();
                    for (k, v) in m.into_iter() {
                        let k = ruby.str_new(&k);
                        let v = RubyToJson::serialize_baml(ruby, types, v)?;
                        hash.aset(k, v)?;
                    }
                    Ok(hash.into_value_with(ruby))
                }
                BamlValueWithMeta::List(l, _) => {
                    let arr = ruby.ary_new();
                    for v in l.into_iter() {
                        let v = RubyToJson::serialize_baml(ruby, types, v)?;
                        arr.push(v)?;
                    }
                    Ok(arr.into_value_with(ruby))
                }
            _ => serde_magnus::serialize(&from),
            }

        }
    }

    pub fn serialize(ruby: &Ruby, types: RModule, from: Value) -> crate::Result<Value> {
        let json = RubyToJson::convert(from)?;
        RubyToJson::serialize_baml(ruby, types, BamlValueWithMeta::with_default_meta(&json))
    }

    /// Convert a Ruby object to a JSON object.
    ///
    /// We have to implement this ourselves instead of relying on Serde, because in the codegen,
    /// we can't convert a BAML-generated type to a hash trivially (specifically union-typed
    /// fields do not serialize correctly, see https://sorbet.org/docs/tstruct#serialize-gotchas)
    ///
    /// We do still rely on :serialize for enums though.
    pub fn convert(from: Value) -> crate::Result<BamlValue> {
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

    pub fn convert_hash_to_json(from: RHash) -> crate::Result<IndexMap<String, BamlValue>> {
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
    ) -> Result<BamlValue, Vec<SerializationError>> {
        if any.is_nil() {
            return Ok(BamlValue::Null);
        }

        if any.is_kind_of(self.ruby.class_true_class()) {
            return Ok(BamlValue::Bool(true));
        }

        if any.is_kind_of(self.ruby.class_false_class()) {
            return Ok(BamlValue::Bool(false));
        }

        if let Some(any) = magnus::Integer::from_value(any) {
            return self.to_int(any, field_pos);
        }

        if let Some(any) = magnus::Float::from_value(any) {
            return self.to_float(any, field_pos);
        }

        if let Some(any) = RString::from_value(any) {
            return self.to_string(any, field_pos).map(BamlValue::String);
        }

        if let Some(any) = RArray::from_value(any) {
            return self.to_array(any, field_pos);
        }

        if let Some(any) = RHash::from_value(any) {
            return self.hash_to_map(any, field_pos).map(BamlValue::Map);
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

        if self.is_type::<Audio>(any) {
            return self.to_type::<Audio>(any, field_pos);
        }

        if self.is_type::<Image>(any) {
            return self.to_type::<Image>(any, field_pos);
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
    ) -> Result<BamlValue, Vec<SerializationError>> {
        if let Ok(any) = any.to_i64() {
            return Ok(BamlValue::Int(any));
        }

        return Err(vec![SerializationError {
            position: field_pos,
            message: format!("failed to convert {:?} to i64", any),
        }]);
    }

    fn to_float(&self, any: Float, _: Vec<String>) -> Result<BamlValue, Vec<SerializationError>> {
        return Ok(BamlValue::Float(any.to_f64()));
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
    ) -> Result<BamlValue, Vec<SerializationError>> {
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

        return Ok(BamlValue::List(arr));
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
    ) -> Result<BamlMap<String, BamlValue>, Vec<SerializationError>> {
        use magnus::r_hash::ForEach;

        let mut errs = vec![];
        let mut map = BamlMap::new();
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
    ) -> Result<BamlValue, Vec<SerializationError>> {
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
        let mut map = BamlMap::new();
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

        let fully_qualified_class_name = unsafe { any.class().name() }.into_owned();
        let class_name = match fully_qualified_class_name.rsplit_once("::") {
            Some((_, class_name)) => class_name.to_string(),
            None => fully_qualified_class_name,
        };
        Ok(BamlValue::Class(class_name, map))

        //Ok(BamlValue::Map(map))
    }

    // This codepath is not used right now - it was implemented as a proof-of-concept
    // for serializing structs to JSON, by combining :to_hash with this method. If the
    // implementation of struct_to_map proves to be stable, we can get rid of this.
    #[allow(dead_code)]
    fn struct_to_map_via_to_hash(
        &self,
        any: Result<Value, Error>,
        field_pos: Vec<String>,
    ) -> Result<BamlValue, Vec<SerializationError>> {
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
            return self.hash_to_map(any, field_pos).map(BamlValue::Map);
        }

        return Err(vec![SerializationError {
            position: field_pos,
            message: format!(
                "struct did not respond to :to_hash with a hash, was: {:#?}",
                any
            ),
        }]);
    }

    fn is_type<T: TypedData>(&self, any: Value) -> bool {
        any.class()
            .eql(T::class(&Ruby::get_with(any)))
            .is_ok_and(|is_eql| is_eql)
    }

    fn to_type<T: TypedData + types::media::CloneAsBamlValue>(
        &self,
        any: Value,
        field_pos: Vec<String>,
    ) -> Result<BamlValue, Vec<SerializationError>> {
        match Obj::<T>::try_convert(any) {
            Ok(o) => Ok(o.clone_as_baml_value()),
            Err(e) => Err(vec![SerializationError {
                position: field_pos,
                message: format!("failed to convert {}: {:#?}", any.class(), e),
            }]),
        }
    }

    fn sorbet_to_json(
        &self,
        any: Value,
        field_pos: Vec<String>,
    ) -> Result<BamlValue, Vec<SerializationError>> {
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
