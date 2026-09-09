// Add alongside the shared interfaces fixture when generating this probe SDK.
interface FacadeSource {
    type Output
    function read(self) -> Self.Output throws never
    function echo<U>(self, value: U) -> U throws never { value }
    function same(self) -> Self throws never { self }
}

class FacadeBox<T> {
    value: T,
    function replace(self, value: T, marker: int = 0) -> T throws never {
        self.value = value;
        self.value
    }
    implements FacadeSource {
        type Output = T
        function read(self) -> T throws never {
            self.value
        }
    }
}

class FacadeRecord<T> {
    value: T,
}

function facade_box() -> FacadeBox<string> throws never {
    FacadeBox<string> { value: "Ada" }
}

function facade_record(value: FacadeBox<string>) -> FacadeRecord<FacadeBox<string>> throws never {
    FacadeRecord<FacadeBox<string>> { value }
}

enum FacadeFlavor {
    Vanilla,
    Chocolate,
}

class EvidenceItem {
    value: string,
}

class EvidenceItemType {
    value: string,
}

class _declareType {
    value: string,
}

class _BamlNoInfer {
    value: string,
}
