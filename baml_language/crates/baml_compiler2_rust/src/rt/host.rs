use super::*;

pub(super) fn emit(
    out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
    string_result: bool,
) {
    emit_host(out, entry, builtins, format_site, None, has_globals, string_result);
}

pub(super) fn emit_benchmark(
    out: &mut String,
    entry: usize,
    setup: usize,
    run: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
    string_result: bool,
) {
    emit_host(
        out,
        entry,
        builtins,
        format_site,
        Some((setup, run)),
        has_globals,
        string_result,
    );
}

fn emit_host(
    mut out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    benchmark: Option<(usize, usize)>,
    has_globals: bool,
    string_result: bool,
) {
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    let benchmark_fn = if benchmark.is_some() {
        "Some(benchmark)"
    } else {
        "None"
    };
    let initializer = if has_globals {
        "Some(initialize_program)"
    } else {
        "None"
    };
    let result_type = if string_result { "String" } else { "i64" };
    let entry_result = if string_result {".map(|value|rt::strings::Str::from_raw(value).as_str(&rt::strings::Cx::current()).to_owned())"} else {""};
    let benchmark_loop = if string_result {"benchmark_string_loop"} else {"benchmark_loop"};
    write!(&mut out,"fn entry()->R<{result_type}>{{unsafe{{let cx=rt::calls::Context::current();if rt::telemetry::active(){{f{entry}::<true>(cx,0)}}else{{f{entry}::<false>(cx,0)}}{entry_result}}}}}\nfn main()->std::process::ExitCode{{{builtin_init}rt::driver_configured(DESCRIPTORS,entry,{benchmark_fn},TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}),{initializer})}}\n").unwrap();
    if let Some((setup, run)) = benchmark {
        write!(&mut out, "fn benchmark(jobs:usize,warmup:usize,elapsed:&std::cell::Cell<u128>)->R<{result_type}>{{unsafe{{let cx=rt::calls::Context::current();if rt::telemetry::active(){{let state=f{setup}::<true>(cx,1)?;rt::{benchmark_loop}(||f{run}::<true>(cx,2,state),jobs,warmup,elapsed)}}else{{let state=f{setup}::<false>(cx,1)?;rt::{benchmark_loop}(||f{run}::<false>(cx,2,state),jobs,warmup,elapsed)}}}}}}\n").unwrap();
    }
}
