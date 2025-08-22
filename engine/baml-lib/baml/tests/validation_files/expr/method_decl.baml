class User {
    name string
    age int

    function Greet(self) -> string {
        "Hello"
    }

    function IsAdult(self) -> bool {
        self.age >= 18
    }

    // TODO: left side of assignment is not an identifier: FieldAccess
    // function SetName(self, name: string) -> bool {
    //     self.name = name;
    //     true
    // }
}
