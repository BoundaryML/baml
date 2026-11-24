function long_list(defaults: string[], overrides: string[]) -> string[] {
    [...defaults, "a fairly long literal element that pushes the list past the width", ...overrides, "tail"]
}

function long_map(defaults: map<string, string>, overrides: map<string, string>) -> map<string, string> {
    { ...defaults, "name": "a fairly long literal value that pushes the map past the width", ...overrides }
}

function commented(defaults: map<string, int>, extra: int[]) -> int {
    let merged = {
        // start from the defaults
        ...defaults,
        "limit": 10, // then pin the limit

        /* callers may add more */ ...more(),
    };
    let list = [
        // keep the caller's order
        ...extra, // then the sentinel
        0,
    ];
    merged.length() + list.length()
}
