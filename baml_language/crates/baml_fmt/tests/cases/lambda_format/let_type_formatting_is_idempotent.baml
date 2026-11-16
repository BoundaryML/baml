function f(t: reflect.Type, value: int) -> int {
    let type T=t
    let type B   extends   Named&Tagged<int> = pkg.get_class("C").as_type();
    let type E extends Named = t else {   return 0 }
    let type C extends Named = t // keep this
    let annotated: Wrapper<T>? = null
    match (value) {
        Wrapper<T> => 1,
        _ => 0
    }
}
