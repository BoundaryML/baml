// Run under `baml run --hot`. Prints what `Step` returns until asked to stop,
// so a test can edit this file and watch the running program change.
function Step() -> string { "v1" }

function main() -> null {
    let last = "";
    while (!baml.fs.exists("stop")) {
        let now = Step();
        if (now != last) {
            log.info(now);
            last = now;
        }
        baml.sys.sleep(baml.time.Duration.from_milliseconds(20));
    }
    null
}
