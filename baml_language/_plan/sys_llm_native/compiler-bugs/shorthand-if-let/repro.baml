function f(v: string?) -> string {
    if let key: string = v {
        let params: map<string, string> = { "key": key };
        params.get("key") ?? ""
    } else {
        ""
    }
}
function g(v: string?) -> string {
    if let other: string = v {
        let params: map<string, string> = { "key": other };
        params.get("key") ?? ""
    } else {
        ""
    }
}
