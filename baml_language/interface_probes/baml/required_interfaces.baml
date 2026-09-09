interface RequiredBase<T> {
    type Output
    type Error = never
    function apply(self, value: T) -> Self.Output throws Self.Error
    function echo<U>(self, value: U) -> U throws never { value }
}

interface RequiredMiddle requires RequiredBase<string, Output=Self.Value, Error=Self.Failure> {
    type Value
    type Failure
}

interface RequiredRoot<T> requires RequiredMiddle<Value=T, Failure=never> {}

interface RequiredUnpinned requires RequiredBase<string> {}

class RequiredText {
    implements RequiredBase<string> {
        type Output = string
        function apply(self, value: string) -> string throws never {
            value
        }
    }
    implements RequiredMiddle {
        type Value = string
        type Failure = never
    }
    implements RequiredRoot<string> {}
    implements RequiredUnpinned {}
}

function required_root() -> RequiredRoot<string> throws never {
    RequiredText {  }
}

function required_middle() -> RequiredMiddle<Value = string, Failure = never> throws never {
    RequiredText {  }
}

function required_unpinned() -> RequiredUnpinned throws never {
    RequiredText {  }
}

function use_required_base(
    value: RequiredBase<string, Output = string, Error = never>,
) -> string throws never {
    value.apply("Ada")
}
