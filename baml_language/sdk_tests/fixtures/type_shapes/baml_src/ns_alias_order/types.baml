type AliasChainHead = AliasChainTail[]
type AliasChainTail = Result
type Result = Success | Failure
type AliasMap = map<string, AliasChainTail>
type EnumResult = Status | Failure
type GenericResult = Box<AliasChainHead>
type RecursiveContainer = RecursiveResult[]
type RecursiveResult = int | Success | RecursiveResult[]
type MutualA = int | MutualB[]
type MutualB = string | MutualA[]

class Success {}

class Failure {}

enum Status {
  Ready,
}

class Box<T> {
  value T
}

class UsesResult {
  value Result
}
