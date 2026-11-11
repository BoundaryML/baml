//! Owned host boundary. Validate the entire request before managed allocation
//! or execution; heap addresses never escape in successful results.
use crate::{
    Dyn, Error, R,
    strings::{Cx, Str},
};

pub use serde_json::Value as Owned;
pub const MAX_DEPTH: usize = 512;

pub enum Type {
    Null,
    Int,
    Float,
    Bool,
    String,
    IntLiteral(i64),
    FloatLiteral(u64),
    BoolLiteral(bool),
    StringLiteral(&'static str),
    Optional(&'static Type),
    Class(&'static Class),
    List {
        element: &'static Type,
        desc: u32,
        growing: bool,
    },
}
pub struct Class {
    pub name: &'static str,
    pub desc: u32,
    pub bytes: usize,
    pub fields: &'static [Field],
}
pub struct Field {
    pub name: &'static str,
    pub ty: &'static Type,
    pub offset: usize,
    pub slot: baml_rt_gc::SlotKind,
}
pub struct Argument {
    pub name: &'static str,
    pub ty: &'static Type,
}
pub struct Function {
    pub name: &'static str,
    pub arguments: &'static [Argument],
    pub invoke: unsafe fn(&[Owned]) -> R<Owned>,
}

impl Type {
    fn pointer(&self) -> bool {
        match self {
            Self::String | Self::StringLiteral(_) | Self::Class(_) | Self::List { .. } => true,
            Self::Optional(inner) => inner.pointer(),
            _ => false,
        }
    }
    pub fn validate(&self, value: &Owned, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("argument nesting exceeds 512".into());
        }
        let valid = match self {
            Self::IntLiteral(expected) => value.as_i64() == Some(*expected),
            Self::FloatLiteral(expected) => value
                .as_f64()
                .is_some_and(|v| (f64::from_bits(*expected) - v).abs() < f64::EPSILON),
            Self::BoolLiteral(expected) => value.as_bool() == Some(*expected),
            Self::StringLiteral(expected) => value.as_str() == Some(*expected),
            Self::Null => value.is_null(),
            Self::Int => value
                .as_i64()
                .is_some_and(|v| baml_type::Int63::new(v).is_some()),
            Self::Float => value.as_f64().is_some_and(f64::is_finite),
            Self::Bool => value.is_boolean(),
            Self::String => value.is_string(),
            Self::Optional(ty) => {
                return if value.is_null() {
                    Ok(())
                } else {
                    ty.validate(value, depth)
                };
            }
            Self::Class(class) => {
                let Some(object) = value.as_object() else {
                    return Err(format!("expected {}", class.name));
                };
                for field in class.fields {
                    match object.get(field.name) {
                        Some(v) => field.ty.validate(v, depth + 1)?,
                        None if matches!(field.ty, Type::Optional(_)) => {}
                        None => {
                            return Err(format!(
                                "missing field {:?} on {}",
                                field.name, class.name
                            ));
                        }
                    }
                }
                // The stdlib's structural JSON decoder ignores unclaimed
                // object fields. Top-level call arguments are checked separately.
                true
            }
            Self::List { element, .. } => {
                let Some(values) = value.as_array() else {
                    return Err("expected array".into());
                };
                for value in values {
                    element.validate(value, depth + 1)?;
                }
                true
            }
        };
        if valid {
            Ok(())
        } else {
            Err("argument does not match the declared type".into())
        }
    }
    /// # Safety
    /// `value` has passed validation and the owner-thread root is active.
    pub unsafe fn hydrate(&self, value: &Owned) -> R<Dyn> {
        let payload = match self {
            Self::Null => return Ok(Dyn { tag: 2, payload: 0 }),
            Self::Int | Self::IntLiteral(_) => Dyn {
                tag: 0,
                payload: value.as_i64().expect("validated int") as u64,
            },
            Self::Float | Self::FloatLiteral(_) => Dyn {
                tag: crate::scalars::FLOAT_TAG,
                payload: value.as_f64().expect("validated float").to_bits(),
            },
            Self::Bool | Self::BoolLiteral(_) => Dyn {
                tag: 1,
                payload: u64::from(value.as_bool().expect("validated bool")),
            },
            Self::String | Self::StringLiteral(_) => Dyn {
                tag: 3,
                payload: unsafe { Cx::current() }
                    .string(value.as_str().expect("validated string"))?
                    .as_raw() as u64,
            },
            Self::Optional(ty) => {
                return if value.is_null() {
                    Ok(Dyn { tag: 2, payload: 0 })
                } else {
                    unsafe { ty.hydrate(value) }
                };
            }
            Self::Class(class) => {
                let object = unsafe { crate::try_alloc(class.desc, class.bytes) }?;
                for field in class.fields {
                    let input = value.get(field.name).unwrap_or(&Owned::Null);
                    let value = unsafe { field.ty.hydrate(input) }?;
                    unsafe { store(object, object.add(field.offset), field.slot, value) };
                }
                Dyn {
                    tag: u64::from(class.desc) + 3,
                    payload: object as u64,
                }
            }
            Self::List {
                element,
                desc,
                growing,
            } => {
                let values = value.as_array().expect("validated list");
                let bytes = values
                    .len()
                    .checked_mul(8)
                    .and_then(|b| b.checked_add(16))
                    .ok_or_else(|| crate::allocation_failure(usize::MAX))?;
                let buffer = unsafe { crate::try_alloc(desc + u32::from(*growing), bytes) }?;
                unsafe { buffer.add(8).cast::<usize>().write(values.len()) };
                let object = if *growing {
                    let object = unsafe { crate::try_alloc(*desc, 32) }?;
                    unsafe {
                        baml_rt_gc::write_ptr(object, object.add(8).cast(), buffer as usize);
                        object.add(16).cast::<usize>().write(values.len());
                        object.add(24).cast::<usize>().write(values.len());
                    }
                    object
                } else {
                    buffer
                };
                for (i, value) in values.iter().enumerate() {
                    let value = unsafe { element.hydrate(value) }?;
                    let slot = unsafe { buffer.add(16 + i * 8) };
                    unsafe {
                        store(
                            buffer,
                            slot,
                            if element.pointer() {
                                baml_rt_gc::SlotKind::Pointer
                            } else {
                                baml_rt_gc::SlotKind::Scalar
                            },
                            value,
                        )
                    };
                }
                Dyn {
                    tag: u64::from(*desc) + 3,
                    payload: object as u64,
                }
            }
        };
        Ok(payload)
    }

    /// Copy a typed return to host storage before releasing the task's roots.
    /// # Safety
    /// The schema and value belong to the running pack.
    pub unsafe fn copy_out(&self, value: Dyn, depth: usize) -> R<Owned> {
        if depth > MAX_DEPTH {
            return Err(Error::internal("return nesting exceeds 512".into()));
        }
        match self {
            Self::Optional(inner) => {
                if value.tag == 2 {
                    Ok(Owned::Null)
                } else {
                    unsafe { inner.copy_out(value, depth) }
                }
            }
            Self::Class(class) => {
                let mut result = serde_json::Map::new();
                let object = value.payload as *const u8;
                for field in class.fields {
                    let value = unsafe { field.ty.read(object.add(field.offset), field.slot) };
                    result.insert(field.name.into(), unsafe {
                        field.ty.copy_out(value, depth + 1)
                    }?);
                }
                Ok(Owned::Object(result))
            }
            Self::List {
                element, growing, ..
            } => {
                let object = value.payload as *const u8;
                let len = unsafe {
                    object
                        .add(if *growing { 16 } else { 8 })
                        .cast::<usize>()
                        .read()
                };
                let buffer = if *growing {
                    unsafe { object.add(8).cast::<*const u8>().read() }
                } else {
                    object
                };
                let mut result = Vec::with_capacity(len);
                for i in 0..len {
                    let value = unsafe {
                        element.read(buffer.add(16 + i * 8), baml_rt_gc::SlotKind::Scalar)
                    };
                    result.push(unsafe { element.copy_out(value, depth + 1) }?);
                }
                Ok(Owned::Array(result))
            }
            _ => unsafe { copy_primitive(value) },
        }
    }
    unsafe fn read(&self, slot: *const u8, kind: baml_rt_gc::SlotKind) -> Dyn {
        if matches!(kind, baml_rt_gc::SlotKind::Tagged) {
            return unsafe { slot.cast::<Dyn>().read() };
        }
        let payload = unsafe { slot.cast::<u64>().read() };
        if let Self::Optional(inner) = self {
            return if payload == 0 {
                Dyn { tag: 2, payload: 0 }
            } else {
                unsafe { inner.read(slot, kind) }
            };
        }
        let tag = match self {
            Self::Null => 2,
            Self::Int | Self::IntLiteral(_) => 0,
            Self::Bool | Self::BoolLiteral(_) => 1,
            Self::Float | Self::FloatLiteral(_) => crate::scalars::FLOAT_TAG,
            Self::String | Self::StringLiteral(_) => 3,
            Self::Class(c) => u64::from(c.desc) + 3,
            Self::List { desc, .. } => u64::from(*desc) + 3,
            Self::Optional(_) => unreachable!(),
        };
        Dyn { tag, payload }
    }
}
unsafe fn store(object: *mut u8, slot: *mut u8, kind: baml_rt_gc::SlotKind, value: Dyn) {
    unsafe {
        match kind {
            baml_rt_gc::SlotKind::Tagged => crate::write_dyn(object, slot.cast(), value),
            baml_rt_gc::SlotKind::Pointer => {
                baml_rt_gc::write_ptr(object, slot.cast(), value.payload as usize)
            }
            baml_rt_gc::SlotKind::Scalar => slot.cast::<u64>().write(value.payload),
        }
    }
}

/// Copy a primitive return while the instance still owns its managed objects.
/// # Safety
/// `value` must be live on the running task's stack.
pub unsafe fn copy_primitive(value: Dyn) -> R<Owned> {
    Ok(match value.tag {
        0 => Owned::from(value.payload.cast_signed()),
        1 => Owned::Bool(value.payload != 0),
        2 => Owned::Null,
        3 => Owned::String(
            unsafe { Str::from_raw(value.payload as usize) }
                .try_as_str(&unsafe { Cx::current() })?
                .to_owned(),
        ),
        crate::scalars::FLOAT_TAG => serde_json::Number::from_f64(f64::from_bits(value.payload))
            .map(Owned::Number)
            .ok_or_else(|| {
                Error::internal("non-finite float cannot be serialized as JSON".into())
            })?,
        _ => {
            return Err(Error::internal(
                "host return requires a generated type adapter".into(),
            ));
        }
    })
}

impl Function {
    pub fn prepare(&self, arguments: Owned) -> Result<Vec<Owned>, String> {
        let Owned::Object(mut arguments) = arguments else {
            return Err("arguments must be a JSON object".into());
        };
        let mut prepared = Vec::with_capacity(self.arguments.len());
        for arg in self.arguments {
            let value = match arguments.remove(arg.name) {
                Some(v) => v,
                None => return Err(format!("missing argument {:?}", arg.name)),
            };
            arg.ty
                .validate(&value, 0)
                .map_err(|e| format!("argument {:?}: {e}", arg.name))?;
            prepared.push(value);
        }
        if let Some(name) = arguments.keys().next() {
            return Err(format!("unknown argument {name:?}"));
        }
        Ok(prepared)
    }

    /// The request is completely checked before invoking generated code.
    /// # Safety
    /// The function and descriptors must belong to this instance's pack.
    pub unsafe fn call(
        &'static self,
        instance: &mut crate::Instance,
        arguments: Owned,
    ) -> R<Owned> {
        let prepared = self.prepare(arguments).map_err(Error::internal)?;
        unsafe { instance.run_owned(move || (self.invoke)(&prepared)) }
    }
}

pub fn driver(
    descriptors: &'static [baml_rt_gc::Descriptor],
    function: &'static Function,
    functions: &'static [crate::telemetry::Function],
    sites: &'static [crate::telemetry::Site],
) -> std::process::ExitCode {
    driver_initialized(descriptors, function, functions, sites, None, None)
}
pub fn driver_initialized(
    descriptors: &'static [baml_rt_gc::Descriptor],
    function: &'static Function,
    functions: &'static [crate::telemetry::Function],
    sites: &'static [crate::telemetry::Site],
    initialize: Option<fn(&mut crate::Instance, &mut crate::telemetry::Session) -> R<()>>,
    format_site: Option<usize>,
) -> std::process::ExitCode {
    let result = (|| -> R<()> {
        let mut args = std::env::args().skip(1);
        let mut value = Owned::Object(Default::default());
        let mut jobs = None;
        let mut warmup = 1usize;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--json-args" => {
                    let text = args
                        .next()
                        .ok_or_else(|| Error::internal("--json-args needs a JSON object".into()))?;
                    value =
                        serde_json::from_str(&text).map_err(|e| Error::internal(e.to_string()))?;
                }
                "--jobs" | "--warmup" => {
                    let number = args
                        .next()
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or_else(|| Error::internal(format!("invalid {arg}")))?;
                    if arg == "--jobs" {
                        jobs = Some(number);
                    } else {
                        warmup = number;
                    }
                }
                _ => return Err(Error::internal(format!("unsupported argument {arg}"))),
            }
        }
        // Check the entire request before runtime initialization or user code.
        let prepared = function.prepare(value).map_err(Error::internal)?;
        let pointers = Box::leak(
            [vec![false; 3], vec![true; descriptors.len()]]
                .concat()
                .into_boxed_slice(),
        );
        let mut instance = crate::Instance::new(descriptors, pointers);
        let mut session = crate::telemetry::Session::new(functions, sites)?;
        let call = |instance: &mut crate::Instance, session: &mut crate::telemetry::Session| {
            let args = prepared.clone();
            session.run(|| unsafe { instance.run_owned(move || (function.invoke)(&args)) })
        };
        let result = (|| -> R<()> {
            if let Some(initialize) = initialize {
                initialize(&mut instance, &mut session)?;
            }
            let value = call(&mut instance, &mut session)?;
            let Some(jobs) = jobs else {
                println!("{}", format_result(&mut session, &value, format_site)?);
                return Ok(());
            };
            for _ in 1..warmup {
                if call(&mut instance, &mut session)? != value {
                    return Err(Error::internal("warmup result mismatch".into()));
                }
            }
            let started = std::time::Instant::now();
            for _ in 0..jobs {
                if call(&mut instance, &mut session)? != value {
                    return Err(Error::internal("timed result mismatch".into()));
                }
            }
            let elapsed_ns = started.elapsed().as_nanos();
            let text = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            println!(
                "{}",
                serde_json::json!({"result":text,"jobs":jobs,"elapsed_ns":elapsed_ns})
            );
            Ok(())
        })();
        if result.as_ref().is_err_and(|e| e.class == "InitFailed") {
            session.finish_initialization_failure();
        } else {
            session.finish(&result);
        }
        result
    })();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            crate::language::print_error(error);
            std::process::ExitCode::FAILURE
        }
    }
}

/// The pack host prints a result through `baml.json.stringify`, an observed
/// root of its own. Scalars and strings replay that call; other values print
/// their copied-out JSON unobserved.
fn format_result(
    session: &mut crate::telemetry::Session,
    value: &Owned,
    site: Option<usize>,
) -> R<String> {
    let text = value.to_string();
    let Some(site) = site else {
        return Ok(text);
    };
    let scalar = match value {
        Owned::Null => Dyn { tag: 2, payload: 0 },
        Owned::Bool(b) => Dyn {
            tag: 1,
            payload: u64::from(*b),
        },
        Owned::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => Dyn {
                tag: 0,
                payload: i.cast_unsigned(),
            },
            (None, Some(f)) => Dyn {
                tag: crate::FLOAT_TAG,
                payload: f.to_bits(),
            },
            _ => return Ok(text),
        },
        Owned::String(s) => return session.format_string(s, site),
        _ => return Ok(text),
    };
    session.format_scalar(scalar, text, site)
}

#[cfg(test)]
mod tests {
    use super::*;
    static F: Function = Function {
        name: "checked",
        arguments: &[Argument {
            name: "n",
            ty: &Type::Int,
        }],
        invoke: |_| panic!("invalid preparation executed user code"),
    };
    #[test]
    fn preparation_rejects_all_invalid_requests_before_invocation() {
        for value in [
            serde_json::json!({}),
            serde_json::json!({"n": 4611686018427387904i64}),
            serde_json::json!({"n": "1"}),
            serde_json::json!({"n":1,"extra":2}),
        ] {
            assert!(F.prepare(value).is_err());
        }
        assert_eq!(
            F.prepare(serde_json::json!({"n":-4611686018427387904i64}))
                .unwrap(),
            vec![Owned::from(-4611686018427387904i64)]
        );
    }

    #[test]
    fn class_and_nested_list_returns_are_copied_under_gc_stress() {
        use baml_rt_gc::{Descriptor, Field as GcField, Kind, SlotKind};
        static DESCRIPTORS: &[Descriptor] = &[
            crate::STRING_DESC,
            crate::lists::descriptor::<Str>(4),
            crate::lists::buffer_descriptor::<Str>(5),
            crate::lists::descriptor::<crate::lists::List<Str>>(6),
            crate::lists::buffer_descriptor::<crate::lists::List<Str>>(7),
            Descriptor {
                kind: Kind::Class,
                size: 24,
                ptr_slots: (8, 1),
                tagged_slots: (0, 0),
                elem: None,
                type_id: 8,
                fields: &[
                    GcField {
                        name: "rows",
                        offset: 8,
                        slot: SlotKind::Pointer,
                        type_id: 6,
                    },
                    GcField {
                        name: "number",
                        offset: 16,
                        slot: SlotKind::Scalar,
                        type_id: crate::scalars::FLOAT_TAG as u32,
                    },
                ],
                finalizer: None,
            },
        ];
        static ROWS: Type = Type::List {
            element: &Type::List {
                element: &Type::String,
                desc: 1,
                growing: true,
            },
            desc: 3,
            growing: true,
        };
        static CLASS: Class = Class {
            name: "Rows",
            desc: 5,
            bytes: 24,
            fields: &[
                Field {
                    name: "rows",
                    ty: &ROWS,
                    offset: 8,
                    slot: SlotKind::Pointer,
                },
                Field {
                    name: "number",
                    ty: &Type::Float,
                    offset: 16,
                    slot: SlotKind::Scalar,
                },
            ],
        };
        let mut instance = crate::Instance::new(
            DESCRIPTORS,
            &[false, false, false, true, true, true, true, true, true],
        );
        instance.set_stress(true);
        let input = serde_json::json!({"rows":[["one","two"],[],["three"]],"number":1.25});
        let expected = input.clone();
        let copied = unsafe {
            instance.run_owned(move || {
                let ty = Type::Class(&CLASS);
                ty.validate(&input, 0).map_err(Error::internal)?;
                let value = ty.hydrate(&input)?;
                crate::poll()?;
                ty.copy_out(value, 0)
            })
        }
        .unwrap();
        assert_eq!(copied, expected);
        assert!(instance.stats().collections > 0);
        // No managed pointer survives into the owned result.
        drop(instance);
        assert_eq!(copied["rows"][2][0], "three");
    }
}
