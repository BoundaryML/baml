class User {
    name string
    age int

    function Greet(self) -> string {
        "Hello"
    }

    function IsAdult(self) -> bool {
        self.age >= 18
    }

    function SetName(self, name: string) -> bool {
        self.name = name;
        true
    }
}
