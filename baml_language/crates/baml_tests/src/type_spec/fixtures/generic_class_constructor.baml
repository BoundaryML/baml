class Box<T> { v T }

function f() -> int {
    let b = Box<int> { v: 5 };
     // ^ user.Box<int>
    b.v
 // ^^^ int
}
