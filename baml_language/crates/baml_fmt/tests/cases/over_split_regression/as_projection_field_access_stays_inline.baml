class Animal {
    function name(self) -> string throws never {
        self.as<Dog>.name
    }
}
