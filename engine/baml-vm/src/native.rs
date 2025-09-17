//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use baml_types::{BamlMap, BamlMedia, BamlMediaContent, BamlMediaType};

use crate::{
    vm::{InternalError, Object, ObjectType, Vm, VmError},
    RuntimeError, Value,
};

impl Vm {
    /// Array length.
    pub fn array_len(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Array;
        let ob_index = self.objects.as_object(&args[0], expected)?;

        let Object::Array(array) = &self.objects[ob_index] else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(&self.objects[ob_index]).into(),
            }
            .into());
        };

        Ok(Value::Int(array.len() as i64))
    }
}

impl Vm {
    /// Length of map
    pub fn map_len(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Map;
        let ob_index = self.objects.as_object(&args[0], expected)?;

        let Object::Map(map) = &self.objects[ob_index] else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(&self.objects[ob_index]).into(),
            }
            .into());
        };

        Ok(Value::Int(map.len() as i64))
    }
    /// Map `contains`
    pub fn map_has(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Map;
        let ob_index = self.objects.as_object(&args[0], expected)?;

        let Object::Map(map) = &self.objects[ob_index] else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(&self.objects[ob_index]).into(),
            }
            .into());
        };

        let key = self.objects.as_string(&args[1])?;

        Ok(Value::Bool(map.contains_key(key)))
    }
}

impl Vm {
    pub fn image_from_url(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let url = self.objects.as_string(&args[0])?;

        Ok(self.alloc_media(BamlMedia::url(BamlMediaType::Image, url.to_owned(), None)))
    }

    pub fn audio_from_url(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let url = self.objects.as_string(&args[0])?;

        Ok(self.alloc_media(BamlMedia::url(BamlMediaType::Audio, url.to_owned(), None)))
    }

    pub fn video_from_url(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let url = self.objects.as_string(&args[0])?;

        Ok(self.alloc_media(BamlMedia::url(BamlMediaType::Video, url.to_owned(), None)))
    }

    pub fn pdf_from_url(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let url = self.objects.as_string(&args[0])?;

        Ok(self.alloc_media(BamlMedia::url(BamlMediaType::Pdf, url.to_owned(), None)))
    }

    pub fn image_from_base64(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media_type = self.objects.as_string(&args[0])?;
        let base64 = self.objects.as_string(&args[1])?;

        Ok(self.alloc_media(BamlMedia::base64(
            BamlMediaType::Image,
            base64.to_owned(),
            Some(media_type.to_owned()),
        )))
    }

    pub fn audio_from_base64(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.5a
        let media_type = self.objects.as_string(&args[0])?;
        let base64 = self.objects.as_string(&args[1])?;

        Ok(self.alloc_media(BamlMedia::base64(
            BamlMediaType::Audio,
            base64.to_owned(),
            Some(media_type.to_owned()),
        )))
    }

    pub fn video_from_base64(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media_type = self.objects.as_string(&args[0])?;
        let base64 = self.objects.as_string(&args[1])?;

        Ok(self.alloc_media(BamlMedia::base64(
            BamlMediaType::Video,
            base64.to_owned(),
            Some(media_type.to_owned()),
        )))
    }

    pub fn pdf_from_base64(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let base64 = self.objects.as_string(&args[0])?;

        Ok(self.alloc_media(BamlMedia::base64(
            BamlMediaType::Pdf,
            base64.to_owned(),
            Some("application/pdf".to_string()),
        )))
    }

    pub fn media_is_url(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media = self.objects.as_media(&args[0])?;

        Ok(Value::Bool(matches!(
            media.content,
            BamlMediaContent::Url(_)
        )))
    }

    pub fn media_is_base64(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media = self.objects.as_media(&args[0])?;

        Ok(Value::Bool(matches!(
            media.content,
            BamlMediaContent::Base64(_)
        )))
    }

    pub fn media_as_url(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media = self.objects.as_media(&args[0])?;

        match &media.content {
            BamlMediaContent::Url(url) => Ok(self.alloc_string(url.url.clone())),

            _ => Err(VmError::RuntimeError(RuntimeError::Other(
                "Media is not a URL".to_string(),
            ))),
        }
    }

    pub fn media_as_base64(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media = self.objects.as_media(&args[0])?;

        match &media.content {
            BamlMediaContent::Base64(base64) => Ok(self.alloc_string(base64.base64.clone())),

            _ => Err(VmError::RuntimeError(RuntimeError::Other(
                "Media is not base64".to_string(),
            ))),
        }
    }

    pub fn media_mime_type(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let media = self.objects.as_media(&args[0])?;

        Ok(self.alloc_string(media.mime_type.clone().unwrap_or("".to_string())))
    }

    pub fn deep_copy_object(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.
        let index = self.objects.as_object(&args[0], ObjectType::Any)?;

        let object = self.objects[index].clone();

        Ok(Value::Object(self.objects.insert(object)))
    }

    pub fn any_value_to_string(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        fn format_value_recursive(
            vm: &mut Vm,
            value: &Value,
            depth: usize,
        ) -> Result<String, VmError> {
            // Check available stack space (MAX_FRAMES - current_frames)
            let available_frames = crate::vm::MAX_FRAMES.saturating_sub(vm.frames.len());

            if depth >= available_frames {
                return Err(VmError::RuntimeError(RuntimeError::StackOverflow));
            }

            match value {
                Value::Null => Ok("null".to_string()),
                Value::Int(i) => Ok(i.to_string()),
                Value::Float(f) => Ok(f.to_string()),
                Value::Bool(b) => Ok(b.to_string()),

                Value::Object(obj_idx) => match &vm.objects[*obj_idx] {
                    Object::Instance(instance) => {
                        let Object::Class(class) = &vm.objects[instance.class] else {
                            return Err(VmError::RuntimeError(RuntimeError::Other(
                                "Invalid class reference".to_string(),
                            )));
                        };

                        let class_name = class.name.clone();
                        let field_names = class.field_names.clone();
                        let fields = instance.fields.clone();

                        let mut result = format!("{class_name} {{\n");
                        let field_indent = "    ".repeat(depth + 1);

                        for (i, field_value) in fields.iter().enumerate() {
                            let field_name = match field_names.get(i) {
                                Some(name) => name.as_str(),
                                None => {
                                    let fallback = format!("field_{i}");
                                    let formatted_value =
                                        format_value_recursive(vm, field_value, depth + 1)?;
                                    result.push_str(&format!(
                                        "{field_indent}{fallback}: {formatted_value}\n"
                                    ));
                                    continue;
                                }
                            };
                            let formatted_value =
                                format_value_recursive(vm, field_value, depth + 1)?;
                            result.push_str(&format!(
                                "{field_indent}{field_name}: {formatted_value}\n"
                            ));
                        }

                        let indent = "    ".repeat(depth);
                        result.push_str(&format!("{indent}}}"));
                        Ok(result)
                    }

                    Object::Array(values) => {
                        let values = values.clone();
                        let mut result = String::from("[");
                        for (i, value) in values.iter().enumerate() {
                            if i > 0 {
                                result.push_str(", ");
                            }
                            result.push_str(&format_value_recursive(vm, value, depth)?);
                        }
                        result.push(']');
                        Ok(result)
                    }

                    Object::Map(map) => {
                        let map = map.clone();
                        let mut result = String::from("{\n");
                        let field_indent = "    ".repeat(depth + 1);

                        for (key, value) in map.iter() {
                            let formatted_value = format_value_recursive(vm, value, depth + 1)?;
                            result
                                .push_str(&format!("{field_indent}\"{key}\": {formatted_value}\n"));
                        }

                        let indent = "    ".repeat(depth);
                        result.push_str(&format!("{indent}}}"));
                        Ok(result)
                    }

                    Object::String(s) => Ok(format!("\"{s}\"")),
                    Object::Enum(e) => Ok(e.name.clone()),
                    Object::Variant(variant) => {
                        let Object::Enum(enm) = &vm.objects[variant.enm] else {
                            return Err(VmError::RuntimeError(RuntimeError::Other(
                                "Invalid enum reference".to_string(),
                            )));
                        };

                        let variant_name = match enm.variant_names.get(variant.index) {
                            Some(name) => name.clone(),
                            None => format!("variant_{}", variant.index),
                        };
                        Ok(variant_name)
                    }
                    Object::Function(f) => Ok(format!("<function {}>", f.name)),
                    Object::Class(c) => Ok(format!("<class {}>", c.name)),
                    Object::Media(_) => Ok("<media>".to_string()),
                    Object::Future(_) => Ok("<future>".to_string()),
                    Object::BamlType(_) => Ok("<baml type>".to_string()),
                },
            }
        }

        let formatted = format_value_recursive(self, &args[0], 0)?;
        Ok(self.alloc_string(formatted))
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

pub fn functions() -> BamlMap<String, (NativeFunction, usize)> {
    let fns: &[(&str, (NativeFunction, usize))] = &[
        // Array.
        ("baml.Array.length", (Vm::array_len, 1)),
        // Map.
        ("baml.Map.length", (Vm::map_len, 1)),
        ("baml.Map.has", (Vm::map_has, 2)),
        // Media
        ("baml.media.image.from_url", (Vm::image_from_url, 1)),
        ("baml.media.audio.from_url", (Vm::audio_from_url, 1)),
        ("baml.media.video.from_url", (Vm::video_from_url, 1)),
        ("baml.media.pdf.from_url", (Vm::pdf_from_url, 1)),
        ("baml.media.image.from_base64", (Vm::image_from_base64, 2)),
        ("baml.media.audio.from_base64", (Vm::audio_from_base64, 2)),
        ("baml.media.video.from_base64", (Vm::video_from_base64, 2)),
        ("baml.media.pdf.from_base64", (Vm::pdf_from_base64, 1)),
        ("baml.media.image.is_url", (Vm::media_is_url, 1)),
        ("baml.media.video.is_url", (Vm::media_is_url, 1)),
        ("baml.media.audio.is_url", (Vm::media_is_url, 1)),
        ("baml.media.pdf.is_url", (Vm::media_is_url, 1)),
        ("baml.media.image.is_base64", (Vm::media_is_base64, 1)),
        ("baml.media.video.is_base64", (Vm::media_is_base64, 1)),
        ("baml.media.audio.is_base64", (Vm::media_is_base64, 1)),
        ("baml.media.pdf.is_base64", (Vm::media_is_base64, 1)),
        ("baml.media.image.as_url", (Vm::media_as_url, 1)),
        ("baml.media.video.as_url", (Vm::media_as_url, 1)),
        ("baml.media.audio.as_url", (Vm::media_as_url, 1)),
        ("baml.media.pdf.as_url", (Vm::media_as_url, 1)),
        ("baml.media.image.as_base64", (Vm::media_as_base64, 1)),
        ("baml.media.video.as_base64", (Vm::media_as_base64, 1)),
        ("baml.media.audio.as_base64", (Vm::media_as_base64, 1)),
        ("baml.media.pdf.as_base64", (Vm::media_as_base64, 1)),
        ("baml.media.image.mime", (Vm::media_mime_type, 1)),
        ("baml.media.video.mime", (Vm::media_mime_type, 1)),
        ("baml.media.audio.mime", (Vm::media_mime_type, 1)),
        ("baml.media.pdf.mime", (Vm::media_mime_type, 1)),
        // Utility functions.
        ("baml.deep_copy", (Vm::deep_copy_object, 1)),
        ("baml.unstable.string", (Vm::any_value_to_string, 1)),
    ];

    BamlMap::from_iter(
        fns.iter()
            .map(|(name, (func, arity))| (name.to_string(), (*func, *arity))),
    )
}
