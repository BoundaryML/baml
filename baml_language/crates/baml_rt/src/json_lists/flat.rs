//! Tokens for generated flat-class array decoders. The entire document is
//! syntax checked first; structural refusals use the diagnostic decoder.
use crate::{Dyn, R, strings::Cx};
use bex_vm_types::json_text::{Mark, Number, Reader, Stop};
#[derive(Clone, Copy)]
pub enum Scalar {
    String,
    Int,
    Float,
    Bool,
    NumberUnion,
    OptionalInt,
    OptionalString,
}
#[derive(Clone, Copy)]
enum Token<'a> {
    Text(&'a str),
    Escaped(Mark),
    Number(Number<'a>),
    Bool(bool),
    Null,
    Other,
}
pub struct Object<'a, const N: usize> {
    reader: Reader<'a>,
    end: Mark,
    fields: [Option<Token<'a>>; N],
}
impl<'a, const N: usize> Object<'a, N> {
    pub fn scan(
        mut reader: Reader<'a>,
        field: impl Fn(&str) -> Option<usize>,
    ) -> Result<Self, Stop> {
        reader.open_object()?;
        let mut fields = [None; N];
        let mut first = true;
        while let Some(key) = reader.next_key(first)? {
            first = false;
            if let Some(slot) = field(key) {
                let next = reader.peek();
                let mark = reader.mark();
                fields[slot] = Some(match next {
                    Some(b'"') => match reader.borrowed_string()? {
                        Some(text) => Token::Text(text),
                        None => Token::Escaped(mark),
                    },
                    Some(b'-' | b'0'..=b'9') => Token::Number(reader.validated_number()?),
                    Some(b't' | b'f') => Token::Bool(reader.bool()?),
                    Some(b'n') => {
                        reader.null()?;
                        Token::Null
                    }
                    _ => {
                        reader.skip_value()?;
                        Token::Other
                    }
                });
            } else {
                reader.skip_value()?;
            }
        }
        let end = reader.mark();
        Ok(Self {
            reader,
            end,
            fields,
        })
    }
    #[inline(always)]
    pub fn matches(&self, index: usize, ty: Scalar) -> bool {
        match (self.fields[index], ty) {
            (None | Some(Token::Null), Scalar::OptionalString | Scalar::OptionalInt) => true,
            (Some(Token::Text(_) | Token::Escaped(_)), Scalar::String | Scalar::OptionalString) => {
                true
            }
            (Some(Token::Number(n)), Scalar::Int | Scalar::OptionalInt) => {
                n.as_i64().and_then(baml_type::Int63::new).is_some()
            }
            (Some(Token::Number(n)), Scalar::Float) => n.as_f64().is_some(),
            (Some(Token::Number(n)), Scalar::NumberUnion) => {
                n.as_i64().and_then(baml_type::Int63::new).is_some() || n.as_f64().is_some()
            }
            (Some(Token::Bool(_)), Scalar::Bool) => true,
            _ => false,
        }
    }
    /// # Safety
    /// `matches` accepted this field and the generated offset/layout agree with
    /// `ty`. The object stays rooted on the native stack through allocations.
    #[inline(always)]
    pub unsafe fn store(
        &mut self,
        cx: &mut Cx,
        object: *mut u8,
        offset: usize,
        index: usize,
        ty: Scalar,
    ) -> R<()> {
        let token = self.fields[index];
        let slot = unsafe { object.add(offset) };
        match ty {
            Scalar::String | Scalar::OptionalString => {
                if matches!(token, None | Some(Token::Null)) {
                    return Ok(());
                }
                let text = match token {
                    Some(Token::Text(s)) => s,
                    Some(Token::Escaped(m)) => self.reader.string_at(m).expect("validated string"),
                    _ => unreachable!(),
                };
                let text = cx.string(text)?;
                unsafe {
                    baml_rt_gc::write_ptr(object, slot.cast(), text.as_raw());
                }
            }
            Scalar::Bool => {
                let Some(Token::Bool(b)) = token else {
                    unreachable!()
                };
                unsafe {
                    slot.cast::<u64>().write(b as u64);
                }
            }
            Scalar::Int => {
                let Some(Token::Number(n)) = token else {
                    unreachable!()
                };
                unsafe {
                    slot.cast::<i64>().write(n.as_i64().unwrap());
                }
            }
            Scalar::Float => {
                let Some(Token::Number(n)) = token else {
                    unreachable!()
                };
                unsafe {
                    slot.cast::<f64>().write(n.as_f64().unwrap());
                }
            }
            Scalar::OptionalInt | Scalar::NumberUnion => {
                let value = match token {
                    None | Some(Token::Null) => Dyn { tag: 2, payload: 0 },
                    Some(Token::Number(n)) => {
                        if let Some(n) = n.as_i64().and_then(baml_type::Int63::new) {
                            Dyn {
                                tag: 0,
                                payload: n.get() as u64,
                            }
                        } else {
                            Dyn {
                                tag: crate::FLOAT_TAG,
                                payload: n.as_f64().unwrap().to_bits(),
                            }
                        }
                    }
                    _ => unreachable!(),
                };
                unsafe {
                    crate::write_dyn(object, slot.cast(), value);
                }
            }
        }
        Ok(())
    }
    pub fn into_reader(mut self) -> Reader<'a> {
        self.reader.reset(self.end);
        self.reader
    }
}
