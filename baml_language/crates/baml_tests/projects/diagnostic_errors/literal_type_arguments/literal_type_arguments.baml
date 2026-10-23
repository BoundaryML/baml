// Unsupported literal arguments must not hide their types or bindings.
type TrueArgs = true<MissingType>
type FalseArgs = false<MissingType>
type TrueBinding = true<Item = MissingType>
type FalseBinding = false<Item = MissingType>
type TrueUnion = int | true<MissingType>
type FalseUnion = int | false<MissingType>
type TrueUnionBinding = int | true<Item = MissingType>
type FalseUnionBinding = int | false<Item = MissingType>
type TrueNested = map<string, (true<MissingType>)[]>
type FalseNested = map<string, (false<Item = MissingType>)[]>

function main() -> TrueArgs throws never { true }
