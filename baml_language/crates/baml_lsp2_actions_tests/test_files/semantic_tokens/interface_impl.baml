// Out-of-body implements: top-level `implements I for T { ... }`

interface ToJson {
    function to_json(self) -> string throws never
}

class Dog {
    breed: string
}

implements ToJson for Dog {
    function to_json(self) -> string {
        return self.breed
    }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// interface_impl.baml:1:1 (comment) len=65 "// Out-of-body implements: top-level `implements I for T { ... }`"
// interface_impl.baml:3:1 (keyword) len=9 "interface"
// interface_impl.baml:3:11 (interface) [declaration] len=6 "ToJson"
// interface_impl.baml:4:5 (keyword) len=8 "function"
// interface_impl.baml:4:14 (method) [declaration] len=7 "to_json"
// interface_impl.baml:4:22 (parameter) [declaration] len=4 "self"
// interface_impl.baml:4:31 (type) [defaultLibrary] len=6 "string"
// interface_impl.baml:4:38 (keyword) len=6 "throws"
// interface_impl.baml:4:45 (type) [defaultLibrary] len=5 "never"
// interface_impl.baml:7:1 (keyword) len=5 "class"
// interface_impl.baml:7:7 (class) [declaration] len=3 "Dog"
// interface_impl.baml:8:5 (property) [declaration] len=5 "breed"
// interface_impl.baml:8:12 (type) [defaultLibrary] len=6 "string"
// interface_impl.baml:11:1 (keyword) len=10 "implements"
// interface_impl.baml:11:12 (interface) len=6 "ToJson"
// interface_impl.baml:11:19 (keyword) len=3 "for"
// interface_impl.baml:11:23 (class) len=3 "Dog"
// interface_impl.baml:12:5 (keyword) len=8 "function"
// interface_impl.baml:12:14 (method) [declaration] len=7 "to_json"
// interface_impl.baml:12:22 (parameter) [declaration] len=4 "self"
// interface_impl.baml:12:31 (type) [defaultLibrary] len=6 "string"
// interface_impl.baml:13:9 (keyword) len=6 "return"
// interface_impl.baml:13:16 (parameter) len=4 "self"
// interface_impl.baml:13:21 (property) len=5 "breed"
