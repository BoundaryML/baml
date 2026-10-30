function classify(v: int | string) -> string {
    if (v is int) {
        "number"
    } else {
        "text"
    }
}
