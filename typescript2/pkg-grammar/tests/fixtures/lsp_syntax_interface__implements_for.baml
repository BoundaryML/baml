// Out-of-body implements: top-level `implements I for T { ... }`

interface ToJson {
    function to_json(self) -> string
}

class Dog {
    breed: string
}

implements ToJson for Dog {
    function to_json(self) -> string {
        return self.breed
    }
}
