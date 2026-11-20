function f(a: int, b: int) -> int {
    let r16 = (a catch (e) { _ => 1 }).b;
    let v20 = a + (b catch (e) { _ => 1 });
    let r18 = (a.b catch (e) { _ => 1 }).c;
    let m = (match (a) { 1 => 2, _ => 3 }).abs();
    r16
}
