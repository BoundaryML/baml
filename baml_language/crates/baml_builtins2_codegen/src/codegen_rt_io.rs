use crate::{BamlType, BuiltinPipeline, NativeBuiltin};
pub fn supports(path: &str) -> bool {
    matches!(
        path,
        "baml.io.print"
            | "baml.io.println"
            | "baml.io.eprint"
            | "baml.io.eprintln"
            | "baml.env.get"
            | "baml.sys.host_target"
            | "baml.sys.pid"
            | "baml.sys.platform"
            | "baml.sys.current_dir"
            | "baml.sys.current_exe"
            | "baml.sys.home_dir"
            | "baml.fs.exists"
            | "baml.fs.remove"
            | "baml.fs.remove_dir"
            | "baml.fs.remove_dir_all"
            | "baml.fs.size"
            | "baml.fs.read"
            | "baml.fs.write"
            | "baml.fs.rename"
            | "baml.fs.canonicalize"
    )
}
pub(crate) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let params = if matches!(d.path.as_str(), "baml.fs.write" | "baml.fs.rename") {
        vec![BamlType::String; 2]
    } else if d.path.starts_with("baml.io.")
        || d.path == "baml.env.get"
        || d.path.starts_with("baml.fs.")
    {
        vec![BamlType::String]
    } else {
        vec![]
    };
    let ret = if d.path == "baml.env.get" || d.path == "baml.sys.home_dir" {
        BamlType::Optional(Box::new(BamlType::String))
    } else if d.path.starts_with("baml.io.")
        || matches!(
            d.path.as_str(),
            "baml.fs.remove" | "baml.fs.remove_dir" | "baml.fs.remove_dir_all" | "baml.fs.rename"
        )
    {
        BamlType::Null
    } else if matches!(
        d.path.as_str(),
        "baml.sys.pid" | "baml.fs.size" | "baml.fs.write"
    ) {
        BamlType::Int
    } else if d.path == "baml.fs.exists" {
        BamlType::Bool
    } else {
        BamlType::String
    };
    if d.pipeline != BuiltinPipeline::Io
        || d.receiver.is_some()
        || !d.generics.is_empty()
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
    {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}",
            d.path
        ));
    }
    let method = match d.path.as_str() {
        "baml.env.get" => "env_get".to_owned(),
        name if name.starts_with("baml.fs.") => format!("fs::{}", name.rsplit('.').next().unwrap()),
        name => name
            .rsplit('.')
            .next()
            .expect("qualified builtin")
            .to_owned(),
    };
    let mut param = (0..params.len())
        .map(|i| format!("a{i}:rt::strings::Str"))
        .collect::<Vec<_>>();
    param.push("call_site:Option<usize>".into());
    let param = param.join(",");
    let arg = (0..params.len())
        .map(|i| format!("a{i}.to_owned()"))
        .collect::<Vec<_>>()
        .join(",");
    let schema = match ret {
        BamlType::Null => "rt::host::Type::Null",
        BamlType::Int => "rt::host::Type::Int",
        BamlType::Bool => "rt::host::Type::Bool",
        BamlType::String => "rt::host::Type::String",
        BamlType::Optional(_) => "rt::host::Type::Optional(&rt::host::Type::String)",
        _ => unreachable!(),
    };
    Ok(format!(
        "#[inline(always)] fn rt_io_{}({param})->rt::R<rt::Dyn>{{let owned=rt::io::perform_at(rt::io::{method}({arg}),call_site)?;let ty={schema};ty.validate(&owned,0).map_err(rt::Error::internal)?;unsafe{{ty.hydrate(&owned)}}}}\n",
        d.fn_name
    ))
}
