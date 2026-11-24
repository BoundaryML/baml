function f(a: Outer?) -> int? {
    let x = (a?.b.upcast<Show>.show()).v;
    let y = (a?.b.m<int>()).v;
    let z = (a?.b.upcast<Show>[0]).v;
    let w = (a?.b).c;
    let kept = ((a?.b)).c;
    let bare = (a.b.upcast<Show>).v;
    x
}
