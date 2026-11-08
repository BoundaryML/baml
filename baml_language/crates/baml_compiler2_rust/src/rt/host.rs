use super::*;

pub(super) fn emit(
    out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    cleanup: bool,
) {
    emit_host(out, entry, builtins, format_site, cleanup, None);
}

pub(super) fn emit_benchmark(
    out: &mut String,
    entry: usize,
    setup: usize,
    run: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    cleanup: bool,
) {
    emit_host(
        out,
        entry,
        builtins,
        format_site,
        cleanup,
        Some((setup, run)),
    );
}

fn emit_host(
    mut out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    cleanup: bool,
    benchmark: Option<(usize, usize)>,
) {
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    let cleanup_init = if cleanup {
        "rt::tasks::set_cleanup_dispatch(cleanup_dispatch);"
    } else {
        ""
    };
    let driver = if benchmark.is_some() {
        "rt::driver_observed_benchmark(DESCRIPTORS,entry,benchmark,"
    } else {
        "rt::driver_observed(DESCRIPTORS,entry,"
    };
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{{cleanup_init}let cx=rt::calls::Context::current();if rt::telemetry::active(){{f{entry}::<true>(cx,0)}}else{{f{entry}::<false>(cx,0)}}}}}}\nfn main()->std::process::ExitCode{{{builtin_init}{driver}TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}))}}\n").unwrap();
    if let Some((setup, run)) = benchmark {
        write!(&mut out, "fn benchmark(jobs:usize,warmup:usize,elapsed:&std::cell::Cell<u128>)->R<i64>{{unsafe{{{cleanup_init}let cx=rt::calls::Context::current();if rt::telemetry::active(){{let state=f{setup}::<true>(cx,1)?;rt::benchmark_loop(||f{run}::<true>(cx,2,state),jobs,warmup,elapsed)}}else{{let state=f{setup}::<false>(cx,1)?;rt::benchmark_loop(||f{run}::<false>(cx,2,state),jobs,warmup,elapsed)}}}}}}\n").unwrap();
    }
}
