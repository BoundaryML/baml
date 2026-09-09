// A JS Promise must not mistake a checked interface ref for a thenable.
interface ThenLike {
    function then(self) -> string throws never
    function close(self) -> string throws never
    function close_(self) -> string throws never
}

class NativeThenLike {
    implements ThenLike {
        function then(self) -> string throws never {
            "then method"
        }
        function close(self) -> string throws never {
            "close method"
        }
        function close_(self) -> string throws never {
            "authored underscore"
        }
    }
}

function make_then_like() -> ThenLike throws never {
    NativeThenLike {  }
}

class _BamlInterfaceRef {
    value: string,
}

class _BamlInterfaceType {
    value: string,
}

class _BamlType {
    value: string,
}

class _BamlConcreteRef {
    value: string,
}

class _interfaceTypes {
    value: string,
}

class _P0 {
    value: string,
}

class _M0 {
    value: string,
}

interface FrameNames<T> {
    function receiver_result(self) -> _P0 throws never
    function method_result<U>(self) -> _M0 throws never
}

class NameOwner {
    implements FrameNames<int> {
        function receiver_result(self) -> _P0 throws never {
            _P0 { value: "receiver" }
        }
        function method_result<U>(self) -> _M0 throws never {
            _M0 { value: "method" }
        }
    }
}

function make_frame_names() -> FrameNames<int> throws never {
    NameOwner {  }
}
