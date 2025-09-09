//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use baml_types::{BamlMap, BamlMedia, BamlMediaType};

use crate::{
    vm::{InternalError, Object, ObjectType, Vm, VmError},
    Value,
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
    pub fn map_contains(&mut self, args: &[Value]) -> Result<Value, VmError> {
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
        // Arity is already checked by the VM.

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
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

pub fn functions() -> BamlMap<String, (NativeFunction, usize)> {
    let fns: &[(&str, (NativeFunction, usize))] = &[
        // Array.
        ("std.Array.len", (Vm::array_len, 1)),
        // Map.
        ("std.Map.len", (Vm::map_len, 1)),
        ("std.Map.contains", (Vm::map_contains, 2)),
        // Media
        ("std.media.image.from_url", (Vm::image_from_url, 1)),
        ("std.media.audio.from_url", (Vm::audio_from_url, 1)),
        ("std.media.video.from_url", (Vm::video_from_url, 1)),
        ("std.media.pdf.from_url", (Vm::pdf_from_url, 1)),
        ("std.media.image.from_base64", (Vm::image_from_base64, 2)),
        ("std.media.audio.from_base64", (Vm::audio_from_base64, 2)),
        ("std.media.video.from_base64", (Vm::video_from_base64, 2)),
        ("std.media.pdf.from_base64", (Vm::pdf_from_base64, 1)),
    ];

    BamlMap::from_iter(
        fns.iter()
            .map(|(name, (func, arity))| (name.to_string(), (*func, *arity))),
    )
}
