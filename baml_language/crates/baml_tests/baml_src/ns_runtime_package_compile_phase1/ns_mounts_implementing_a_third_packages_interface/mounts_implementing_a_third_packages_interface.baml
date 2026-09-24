/// A mounted package may implement an interface declared by a THIRD package:
/// a sibling mount, or the standard library. Its impl rows name that other
/// package's interface, and the mount boundary judges them against that
/// interface wherever this world can see it.
function shapes() -> reflect.Package {
    reflect.Package.compile({ "c.baml": `
        interface Shape {
            function area(self) -> int throws never
            function describe(self) -> string throws never { "shape" }
        }
    ` })
}

/// `Square` adopts the interface's default `describe`; `Circle`, implemented
/// out of body, provides both methods.
function squares(c: reflect.Package) -> reflect.Package {
    reflect.Package.compile(
        { "b.baml": `
            class Square {
                side int
                implements c.Shape {
                    function area(self) -> int throws never { self.side * self.side }
                }
            }
            class Circle { r int }
            implement c.Shape for Circle {
                function area(self) -> int throws never { 3 * self.r * self.r }
                function describe(self) -> string throws never { "circle" }
            }
            function square(side: int) -> Square throws never { Square { side: side } }
            function circle(r: int) -> Circle throws never { Circle { r: r } }
        ` },
        packages = { "c": c },
    )
}

function through_the_sibling_interface() -> string {
    let c = shapes();
    let consumer = reflect.Package.compile(
        { "main.baml": `
            function go() -> string throws never {
                let s: c.Shape = b.square(3);
                let k: c.Shape = b.circle(2);
                s.describe() + ":" + s.area().to_string() + " " + k.describe() + ":" + k.area().to_string()
            }
        ` },
        packages = { "b": squares(c), "c": c },
    );
    let go = consumer.get_function<() -> string>("root.go") ?? throw "the consumer has no `go`";
    go()
}

/// The consumer never mounts `c`: that package is opaque here, so the impl
/// rows naming its interface have nothing to be judged against and nothing
/// here can dispatch through them — the mount is served all the same.
function without_the_third_package() -> int {
    let consumer = reflect.Package.compile(
        { "main.baml": `
            function go() -> int throws never { b.square(3).side }
        ` },
        packages = { "b": squares(shapes()) },
    );
    let go = consumer.get_function<() -> int>("root.go") ?? throw "the consumer has no `go`";
    go()
}

function through_a_standard_library_interface() -> string {
    let app = reflect.Package.compile({ "w.baml": `
        class Widget {
            n int
            implements baml.ToString {
                function to_string(self) -> string throws never { "widget" }
            }
        }
        function widget() -> Widget throws never { Widget { n: 1 } }
    ` });
    let consumer = reflect.Package.compile(
        { "main.baml": `
            function go() -> string throws never {
                let t: baml.ToString = app.widget();
                t.to_string()
            }
        ` },
        packages = { "app": app },
    );
    let go = consumer.get_function<() -> string>("root.go") ?? throw "the consumer has no `go`";
    go()
}

test "mounts_implementing_a_third_packages_interface" {
    assert.equal(through_the_sibling_interface(), "shape:9 circle:12");
    assert.equal(without_the_third_package(), 3);
    assert.equal(through_a_standard_library_interface(), "widget")
}
