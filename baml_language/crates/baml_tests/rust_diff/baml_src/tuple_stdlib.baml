function TupleStdlibEntries() -> (int, int[])[] {
    let values: map<int, int[]> = { 1: [10] };
    values.entries()
}

function TupleStdlib() -> void {
    let values: int?[] = [3, null, 1, null, 2];
    let indexed = values.enumerate().collect();
    let zipped = values.zip(["a", "b", "c"]);
    let (let left, let right) = baml.iter.unzip(zipped);
    let partitioned = values.iter().partition((x: int?) -> bool {
        x != null
    });
    let mapped = baml.Map.from_entries(values.enumerate());
    let entries = mapped.entries();
    let escaped = TupleStdlibEntries();
    await spawn { escaped[0].1.push(20); };
    values.sort_by_key((x: int?) -> int {
        x ?? 0
    });
    baml.io.println(`${indexed}:${left}:${right}:${partitioned}:${entries}:${escaped}:${values}`);
    baml.io.println(`${reflect.Type.of_value(entries[0]) == reflect.Type.of<(int, int?)>()}`);
}
