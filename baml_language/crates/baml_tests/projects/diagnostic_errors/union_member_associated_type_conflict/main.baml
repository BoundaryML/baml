// A member reached through a union is typed per arm. Two values of one union
// can bind an associated type differently, so a position that needs a single
// type (a method parameter, a field) is rejected when the arms disagree
// (#5627: K1's `int` reached K0's `take(p: string)`). A return type is the
// union of what the arms return, so it is rejected only where that union does
// not fit.

interface Slot {
    type Out;
    item: Self.Out,
    function take(self, p: Self.Out) -> string throws never;
    function get(self) -> Self.Out throws never;
}

class Text {
    text: string,
    implements Slot {
        type Out = string;
        item as text
        function take(self, p: string) -> string {
            p
        }
        function get(self) -> string {
            self.text
        }
    }
}

class Count {
    count: int,
    implements Slot {
        type Out = int;
        item as count
        function take(self, p: int) -> string {
            "count"
        }
        function get(self) -> int {
            self.count
        }
    }
}

function parameter_conflict(x: Text | Count, y: Text | Count) -> string {
    x.take(y.get())
}

function field_conflict(x: Text | Count) -> string {
    x.item
}

function return_is_the_union_of_the_arms(x: Text | Count) -> string {
    x.get()
}
