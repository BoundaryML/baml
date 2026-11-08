use super::*;

pub(super) fn emit(
    out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
    returned: Repr,
) {
    emit_host(
        out,
        entry,
        builtins,
        format_site,
        has_globals,
        returned,
        None,
    );
}

pub(super) fn emit_benchmark(
    out: &mut String,
    entry: usize,
    setup: usize,
    run: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
    returned: Repr,
) {
    emit_host(
        out,
        entry,
        builtins,
        format_site,
        has_globals,
        returned,
        Some((setup, run)),
    );
}

fn emit_host(
    mut out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
    returned: Repr,
    benchmark: Option<(usize, usize)>,
) {
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    let host_type = if returned == Repr::Str {
        "String"
    } else {
        "i64"
    };
    let convert = if returned == Repr::Str {
        "rt::strings::Str::from_raw(value).as_str(&rt::strings::Cx::current()).to_owned()"
    } else {
        "value"
    };
    let driver = match (benchmark.is_some(), has_globals) {
        (true, true) => "rt::driver_observed_benchmark_initialized(DESCRIPTORS,entry,benchmark,",
        (true, false) => "rt::driver_observed_benchmark(DESCRIPTORS,entry,benchmark,",
        (false, true) => "rt::driver_observed_initialized(DESCRIPTORS,entry,",
        (false, false) => "rt::driver_observed(DESCRIPTORS,entry,",
    };
    let initialize = if has_globals {
        ",initialize_program"
    } else {
        ""
    };
    write!(&mut out,"fn entry()->R<{host_type}>{{unsafe{{initialize_globals()?;let cx=rt::calls::Context::current();let value=if rt::telemetry::active(){{f{entry}::<true>(cx,0)}}else{{f{entry}::<false>(cx,0)}}?;Ok({convert})}}}}\nfn main()->std::process::ExitCode{{{builtin_init}{driver}TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}){initialize})}}\n").unwrap();
    if let Some((setup, run)) = benchmark {
        write!(&mut out, "fn benchmark(jobs:usize,warmup:usize,elapsed:&std::cell::Cell<u128>)->R<{host_type}>{{unsafe{{initialize_globals()?;let cx=rt::calls::Context::current();if rt::telemetry::active(){{let state=f{setup}::<true>(cx,1)?;rt::benchmark_loop(||{{let value=f{run}::<true>(cx,2,state)?;Ok({convert})}},jobs,warmup,elapsed)}}else{{let state=f{setup}::<false>(cx,1)?;rt::benchmark_loop(||{{let value=f{run}::<false>(cx,2,state)?;Ok({convert})}},jobs,warmup,elapsed)}}}}}}\n").unwrap();
    }
}
