class Animal {
    function name(self) -> string throws never {
        self.upcast<Dog>.name
    }
}
