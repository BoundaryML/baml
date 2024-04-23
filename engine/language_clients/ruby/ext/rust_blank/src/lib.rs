use magnus::{
    class, define_class,
    encoding::{CType, RbEncoding},
    exception::runtime_error,
    function, method,
    prelude::*,
    Error, RObject, RString, Ruby,
};

type Result<T> = std::result::Result<T, magnus::Error>;

fn is_blank(rb_self: RString) -> Result<String> {
    // RString::as_str is unsafe as it's possible for Ruby to invalidate the
    // str as we hold a reference to it, but here we're only ever using the
    // &str before Ruby is invoked again, so it doesn't get a chance to mess
    // with it and this is safe.
    unsafe {
        // fast path, string is valid utf-8 and we can use Rust's stdlib
        if let Some(s) = rb_self.test_as_str() {
            return Ok("first path".to_string());
        }
    }

    // slow path, use Ruby's API to iterate the codepoints and check for blanks
    let enc = RbEncoding::from(rb_self.enc_get());
    // Similar to ::as_str above, RString::codepoints holds a reference to the
    // underlying string data and we can't let Ruby mutate or invalidate the
    // string while we hold a reference to the codepoints iterator. Here we
    // don't invoke any Ruby methods that could modify the string, so this is
    // safe.
    unsafe {
        for cp in rb_self.codepoints() {
            if !enc.is_code_ctype(cp?, CType::Blank) {
                return Ok("second path".to_string());
            }
        }
    }

    Ok("third path".to_string())
}

fn foo(rb_self: magnus::value::Value) -> Result<String> {
    Ok("foo bar fizz buzz".to_string())
}

#[magnus::wrap(class = "BamlRuntimeFfi", free_immediately, size)]
struct BamlRuntime {
    field: String,
}

impl BamlRuntime {
    pub fn new() -> Result<Self> {
        Ok(BamlRuntime {
            field: "internal field".to_string(),
        })
    }

    pub fn latin(&self) -> Result<String> {
        Ok("lorem ipsum".to_string())
    }
}

#[magnus::init]
fn init() -> Result<()> {
    let class = define_class("String", class::object())?;
    class.define_method("blank?", method!(is_blank, 0))?;

    let t = define_class("TrueClass", class::object())?;
    t.define_method("foo?", method!(foo, 0))?;

    let Ok(rb) = Ruby::get() else {
        return Err(Error::new(runtime_error(), "BANG"));
    };

    let runtime_class = rb.define_class("BamlRuntimeFfi", class::object())?;
    runtime_class.define_singleton_method("new", function!(BamlRuntime::new, 0))?;
    runtime_class.define_method("latin", method!(BamlRuntime::latin, 0))?;

    Ok(())
}
