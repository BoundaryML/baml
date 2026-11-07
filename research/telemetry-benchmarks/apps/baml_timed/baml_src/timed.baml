// Times each Classify call inside one process, the way py_apps.py times its calls.
function Timed(text: string, n: int) -> int[] {
    let times: int[] = [];
    while (times.length() < n) {
        let start = baml.time.Instant.now();
        Classify(text);
        times.push(int.parse(start.elapsed().to_nanoseconds().to_string()));
    }
    times
}
