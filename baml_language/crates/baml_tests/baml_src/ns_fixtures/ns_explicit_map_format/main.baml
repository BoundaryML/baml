function maps() -> map<string, int>[] {
    [
        map {},
        map /* prefix */ { /* inside */ },
        map // prefix
        {
            "key": 1,
        },
        map {
            // entry
            "key": 2,
        },
    ]
}
