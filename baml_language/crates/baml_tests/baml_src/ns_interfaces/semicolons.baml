type SemicolonName = string;

interface SemicolonSource {
    type Item;
    type Error = never;

    function get(self) -> Self.Item throws Self.Error;

    function again(self) -> Self.Item throws Self.Error {
        self.get()
    }
}

class SemicolonValue {
    value: SemicolonName,

    implements SemicolonSource {
        type Item = SemicolonName;

        function get(self) -> SemicolonName {
            self.value
        }
    }
}

function semicolon_declarations_main() -> string {
    let source: SemicolonSource<Item = SemicolonName, Error = never> = SemicolonValue { value: "named" };
    source.again()
}

test "semicolon_terminated_interface_declarations" {
    assert.equal(semicolon_declarations_main(), "named")
}
