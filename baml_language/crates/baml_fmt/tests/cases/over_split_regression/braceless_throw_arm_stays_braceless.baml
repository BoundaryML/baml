function f(s: string) -> int throws Boom {
    baml.json.decode<int>(s) catch (e) {
        baml.json.ParseError => throw Boom {},
        baml.json.DecodeError => 0,
    }
}

class Boom {
}
