use minijinja::value::{from_args, ValueKind};
use minijinja::{format_filter, Error, ErrorKind, FormatStyle, State, Value};

/// An unknown method callback implementing python methods on primitives.
///
/// This implements a lot of Python methods on basic types so that the
/// compatibility with Jinja2 templates improves.
///
/// ```
/// use minijinja::Environment;
/// use minijinja_contrib::pycompat::unknown_method_callback;
///
/// let mut env = Environment::new();
/// env.set_unknown_method_callback(unknown_method_callback);
/// ```
///
/// Today the following methods are implemented:
///
/// * `dict.get`
/// * `dict.items`
/// * `dict.keys`
/// * `dict.values`
/// * `list.count`
/// * `str.capitalize`
/// * `str.count`
/// * `str.endswith`
/// * `str.find`
/// * `str.format`
/// * `str.isalnum`
/// * `str.isalpha`
/// * `str.isascii`
/// * `str.isdigit`
/// * `str.islower`
/// * `str.isnumeric`
/// * `str.isupper`
/// * `str.join`
/// * `str.lower`
/// * `str.lstrip`
/// * `str.replace`
/// * `str.rfind`
/// * `str.rstrip`
/// * `str.split`
/// * `str.splitlines`
/// * `str.startswith`
/// * `str.strip`
/// * `str.title`
/// * `str.upper`
#[cfg_attr(docsrs, doc(cfg(feature = "pycompat")))]
pub fn unknown_method_callback(
    _state: &State,
    value: &Value,
    method: &str,
    args: &[Value],
) -> Result<Value, Error> {
    match value.kind() {
        ValueKind::String => string_methods(value, method, args),
        ValueKind::Map => map_methods(value, method, args),
        ValueKind::Seq => seq_methods(value, method, args),
        _ => Err(Error::from(ErrorKind::UnknownMethod)),
    }
}

fn string_methods(value: &Value, method: &str, args: &[Value]) -> Result<Value, Error> {
    let Some(s) = value.as_str() else {
        return Err(Error::from(ErrorKind::UnknownMethod));
    };

    match method {
        "upper" => {
            let () = from_args(args)?;
            Ok(Value::from(s.to_uppercase()))
        }
        "lower" => {
            let () = from_args(args)?;
            Ok(Value::from(s.to_lowercase()))
        }
        "islower" => {
            let () = from_args(args)?;
            Ok(Value::from(s.chars().all(|x| x.is_lowercase())))
        }
        "isupper" => {
            let () = from_args(args)?;
            Ok(Value::from(s.chars().all(|x| x.is_uppercase())))
        }
        "isspace" => {
            let () = from_args(args)?;
            Ok(Value::from(s.chars().all(|x| x.is_whitespace())))
        }
        "isdigit" | "isnumeric" => {
            // this is not a perfect mapping to what Python does, but
            // close enough for most uses in templates.
            let () = from_args(args)?;
            Ok(Value::from(s.chars().all(|x| x.is_numeric())))
        }
        "isalnum" => {
            let () = from_args(args)?;
            Ok(Value::from(s.chars().all(|x| x.is_alphanumeric())))
        }
        "isalpha" => {
            let () = from_args(args)?;
            Ok(Value::from(s.chars().all(|x| x.is_alphabetic())))
        }
        "isascii" => {
            let () = from_args(args)?;
            Ok(Value::from(s.is_ascii()))
        }
        "strip" => {
            let (chars,): (Option<&str>,) = from_args(args)?;
            Ok(Value::from(if let Some(chars) = chars {
                s.trim_matches(&chars.chars().collect::<Vec<_>>()[..])
            } else {
                s.trim()
            }))
        }
        "lstrip" => {
            let (chars,): (Option<&str>,) = from_args(args)?;
            Ok(Value::from(if let Some(chars) = chars {
                s.trim_start_matches(&chars.chars().collect::<Vec<_>>()[..])
            } else {
                s.trim_start()
            }))
        }
        "rstrip" => {
            let (chars,): (Option<&str>,) = from_args(args)?;
            Ok(Value::from(if let Some(chars) = chars {
                s.trim_end_matches(&chars.chars().collect::<Vec<_>>()[..])
            } else {
                s.trim_end()
            }))
        }
        "replace" => {
            let (old, new, count): (&str, &str, Option<i32>) = from_args(args)?;
            let count = count.unwrap_or(-1);
            Ok(Value::from(if count < 0 {
                s.replace(old, new)
            } else {
                s.replacen(old, new, count as usize)
            }))
        }
        "title" => {
            let () = from_args(args)?;
            // one shall not call into these filters.  However we consider ourselves
            // privileged.
            Ok(Value::from(minijinja::filters::title(s.into())))
        }
        "split" => {
            let (sep, maxsplits) = from_args(args)?;
            // one shall not call into these filters.  However we consider ourselves
            // privileged.
            Ok(minijinja::filters::split(s.into(), sep, maxsplits)
                .try_iter()?
                .collect::<Value>())
        }
        "splitlines" => {
            let (keepends,): (Option<bool>,) = from_args(args)?;
            if !keepends.unwrap_or(false) {
                Ok(s.lines().map(Value::from).collect())
            } else {
                let mut rv = Vec::new();
                let mut rest = s;
                while let Some(offset) = rest.find('\n') {
                    rv.push(Value::from(&rest[..offset + 1]));
                    rest = &rest[offset + 1..];
                }
                if !rest.is_empty() {
                    rv.push(Value::from(rest));
                }
                Ok(Value::from(rv))
            }
        }
        "capitalize" => {
            let () = from_args(args)?;
            // one shall not call into these filters.  However we consider ourselves
            // privileged.
            Ok(Value::from(minijinja::filters::capitalize(s.into())))
        }
        "count" => {
            let (what,): (&str,) = from_args(args)?;
            let mut c = 0;
            let mut rest = s;
            while let Some(offset) = rest.find(what) {
                c += 1;
                rest = &rest[offset + what.len()..];
            }
            Ok(Value::from(c))
        }
        "find" => {
            let (what,): (&str,) = from_args(args)?;
            Ok(Value::from(match s.find(what) {
                Some(x) => x as i64,
                None => -1,
            }))
        }
        "format" => format_filter(FormatStyle::StrFormat, s, args).map(Value::from),
        "rfind" => {
            let (what,): (&str,) = from_args(args)?;
            Ok(Value::from(match s.rfind(what) {
                Some(x) => x as i64,
                None => -1,
            }))
        }
        "startswith" => {
            let (prefix,): (&Value,) = from_args(args)?;
            if let Some(prefix) = prefix.as_str() {
                Ok(Value::from(s.starts_with(prefix)))
            } else if matches!(prefix.kind(), ValueKind::Iterable | ValueKind::Seq) {
                for prefix in prefix.try_iter()? {
                    if s.starts_with(prefix.as_str().ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidOperation,
                            format!(
                                "tuple for startswith must contain only strings, not {}",
                                prefix.kind()
                            ),
                        )
                    })?) {
                        return Ok(Value::from(true));
                    }
                }
                Ok(Value::from(false))
            } else {
                Err(Error::new(
                    ErrorKind::InvalidOperation,
                    format!(
                        "startswith argument must be string or a tuple of strings, not {}",
                        prefix.kind()
                    ),
                ))
            }
        }
        "endswith" => {
            let (suffix,): (&Value,) = from_args(args)?;
            if let Some(suffix) = suffix.as_str() {
                Ok(Value::from(s.ends_with(suffix)))
            } else if matches!(suffix.kind(), ValueKind::Iterable | ValueKind::Seq) {
                for suffix in suffix.try_iter()? {
                    if s.ends_with(suffix.as_str().ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidOperation,
                            format!(
                                "tuple for endswith must contain only strings, not {}",
                                suffix.kind()
                            ),
                        )
                    })?) {
                        return Ok(Value::from(true));
                    }
                }
                Ok(Value::from(false))
            } else {
                Err(Error::new(
                    ErrorKind::InvalidOperation,
                    format!(
                        "endswith argument must be string or a tuple of strings, not {}",
                        suffix.kind()
                    ),
                ))
            }
        }
        "join" => {
            use std::fmt::Write;
            let (values,): (&Value,) = from_args(args)?;
            let mut rv = String::new();
            for (idx, value) in values.try_iter()?.enumerate() {
                if idx > 0 {
                    rv.push_str(s);
                }
                write!(rv, "{value}").ok();
            }
            Ok(Value::from(rv))
        }
        _ => Err(Error::from(ErrorKind::UnknownMethod)),
    }
}

fn map_methods(value: &Value, method: &str, args: &[Value]) -> Result<Value, Error> {
    let Some(obj) = value.as_object() else {
        return Err(Error::from(ErrorKind::UnknownMethod));
    };

    match method {
        "keys" => {
            let () = from_args(args)?;
            Ok(Value::make_object_iterable(obj.clone(), |obj| {
                match obj.try_iter() {
                    Some(iter) => iter,
                    None => Box::new(None.into_iter()),
                }
            }))
        }
        "values" => {
            let () = from_args(args)?;
            Ok(Value::make_object_iterable(obj.clone(), |obj| {
                match obj.try_iter_pairs() {
                    Some(iter) => Box::new(iter.map(|(_, v)| v)),
                    None => Box::new(None.into_iter()),
                }
            }))
        }
        "items" => {
            let () = from_args(args)?;
            Ok(Value::make_object_iterable(obj.clone(), |obj| {
                match obj.try_iter_pairs() {
                    Some(iter) => Box::new(iter.map(|(k, v)| Value::from(vec![k, v]))),
                    None => Box::new(None.into_iter()),
                }
            }))
        }
        "get" => {
            let (key, default): (&Value, Option<Value>) = from_args(args)?;
            Ok(match obj.get_value(key) {
                Some(value) => value,
                None => default.unwrap_or_else(|| Value::from(())),
            })
        }
        _ => Err(Error::from(ErrorKind::UnknownMethod)),
    }
}

fn seq_methods(value: &Value, method: &str, args: &[Value]) -> Result<Value, Error> {
    let Some(obj) = value.as_object() else {
        return Err(Error::from(ErrorKind::UnknownMethod));
    };

    match method {
        "count" => {
            let (what,): (&Value,) = from_args(args)?;
            Ok(Value::from(if let Some(iter) = obj.try_iter() {
                iter.filter(|x| x == what).count()
            } else {
                0
            }))
        }
        _ => Err(Error::from(ErrorKind::UnknownMethod)),
    }
}
