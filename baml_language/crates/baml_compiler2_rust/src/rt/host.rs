use super::*;

pub(super) fn emit(
    out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
) {
    emit_host(out, entry, builtins, format_site, None, has_globals);
}

pub(super) fn emit_benchmark(
    out: &mut String,
    entry: usize,
    setup: usize,
    run: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
) {
    emit_host(
        out,
        entry,
        builtins,
        format_site,
        Some((setup, run)),
        has_globals,
    );
}

fn emit_host(
    mut out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    benchmark: Option<(usize, usize)>,
    has_globals: bool,
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
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{let cx=rt::calls::Context::current();if rt::telemetry::active(){{f{entry}::<true>(cx,0)}}else{{f{entry}::<false>(cx,0)}}}}}}\nfn main()->std::process::ExitCode{{{builtin_init}rt::driver_configured(DESCRIPTORS,entry,{benchmark_fn},TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}),{initializer})}}\n").unwrap();
    if let Some((setup, run)) = benchmark {
        write!(&mut out, "fn benchmark(jobs:usize,warmup:usize,elapsed:&std::cell::Cell<u128>)->R<i64>{{unsafe{{let cx=rt::calls::Context::current();if rt::telemetry::active(){{let state=f{setup}::<true>(cx,1)?;rt::benchmark_loop(||f{run}::<true>(cx,2,state),jobs,warmup,elapsed)}}else{{let state=f{setup}::<false>(cx,1)?;rt::benchmark_loop(||f{run}::<false>(cx,2,state),jobs,warmup,elapsed)}}}}}}\n").unwrap();
    }
}
