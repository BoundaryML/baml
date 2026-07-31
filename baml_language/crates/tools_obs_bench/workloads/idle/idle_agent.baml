// obs-bench workload: idle agent (design §10.2 `idle/idle_agent`).
//
// A long-lived process doing almost nothing: one call at start, then parked
// on sleeps, one call per wake. C3's idle gate binds here: steady-state CCT
// bytes <= 44 B/s (computed ~7 B/s — heartbeat watermarks only; no delta
// rows for windows where nothing closed). Also exercises the §5.3 open-call
// accounting: the sleeping sysop accrues awaiting time across many flush
// windows while producing no per-window close rows.
//
// `wakes` x `sleep_ms` sets the idle span (e.g. 60 x 1000 = one minute).

function tick(i: int) -> int {
    i + 1
}

function main(wakes: int, sleep_ms: int) -> int {
    let n = 0;
    let w = 0;
    while (w < wakes) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(0n + sleep_ms));
        n = tick(n);
        w += 1;
    }
    n
}
