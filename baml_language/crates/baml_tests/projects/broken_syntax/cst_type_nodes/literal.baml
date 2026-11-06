type Scalar = true<MissingType>
type InUnion = int | false<MissingType>
type Nested = map<string, true<Item = MissingType>[]>
type AfterLiteral = true | false
