// enclosing locals resolve inside tagged template bodies
//baml:tagged_string
function ttl_render(body: () -> baml.TaggedString) -> string {
    let t = body()
    let out = ""
    let i = 0
    while (i < t.values.length()) {
        let piece = match (t.values[i]) {
            let s: string => s,
            _ => "?"
        }
        out = out + t.parts[i] + piece
        i = i + 1
    }
    out + t.parts[t.values.length()]
}
function ttl_interpolate() -> string {
    let name = "World"
    let text = ttl_render`Hi ${name}!`
     // ^^^^ string
    text
}
function ttl_control() -> string {
    let items = ["a", "b"]
    let flag = true
    let text = ttl_render`${if (flag)}${for (let x in items)}[${x}]${endfor}${endif}`
     // ^^^^ string
    text
}
