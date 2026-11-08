//! Observable error constructors shared by list mutation and serialization.
use crate::{Error, R, strings::Cx};
use baml_rt_gc::{Descriptor, Field, Kind, SlotKind, write_ptr};
pub const SERIALIZATION_DESC: Descriptor = Descriptor {
    kind: Kind::Class,
    size: 32,
    ptr_slots: (8, 3),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 13,
    fields: &[
        Field {
            name: "message",
            offset: 8,
            slot: SlotKind::Pointer,
            type_id: 3,
        },
        Field {
            name: "path",
            offset: 16,
            slot: SlotKind::Pointer,
            type_id: 3,
        },
        Field {
            name: "reason",
            offset: 24,
            slot: SlotKind::Pointer,
            type_id: 3,
        },
    ],
    finalizer: None,
};
pub const INVALID_ARGUMENT_DESC: Descriptor = Descriptor {
    type_id: 14,
    ..crate::OVERFLOW_DESC
};
fn build(cx: &mut Cx, desc: u32, fields: &[&str]) -> R<usize> {
    let pointer = unsafe { crate::try_alloc(desc, 8 + fields.len() * 8) }?;
    for (i, field) in fields.iter().enumerate() {
        let value = cx.string(field)?;
        unsafe {
            write_ptr(pointer, pointer.add(8 + i * 8).cast(), value.as_raw());
        }
    }
    Ok(pointer as usize)
}
pub(crate) fn invalid_argument(cx: &mut Cx, message: String) -> Box<Error> {
    match build(cx, 11, &[&message]) {
        Err(e) => e,
        Ok(value) => Box::new(Error {
            telemetry_raise: None,
            telemetry_at: None,
            value,
            class: "baml.errors.InvalidArgument",
            message,
            frames: vec![],
            display_value: None,
        }),
    }
}
pub(crate) fn serialization(cx: &mut Cx, message: String, path: &str, reason: &str) -> Box<Error> {
    match build(cx, 10, &[&message, path, reason]) {
        Err(e) => e,
        Ok(value) => Box::new(Error {
            telemetry_raise: None,
            telemetry_at: None,
            value,
            class: "baml.json.SerializationError",
            display_value: Some(format!(
                "baml.json.SerializationError {{message: {message:?}, path: {path:?}, reason: {reason:?}}}"
            )),
            message,
            frames: vec![],
        }),
    }
}
