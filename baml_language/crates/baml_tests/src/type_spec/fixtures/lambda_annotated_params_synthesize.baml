function g() -> string {
    let f = (s: string) -> string throws never { s }
     // ^ (s: string) -> string throws never
    f("hi")
 // ^^^^^^^ string
}
