use magnus::{
    class, define_class,
    encoding::{CType, RbEncoding},
    exception::runtime_error,
    function, method,
    prelude::*,
    Error, RObject, RString, Ruby,
};

type Result<T> = std::result::Result<T, magnus::Error>;

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
        Ok("ipsum lorem".to_string())
    }
}

#[magnus::init(name = "baml")]
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
