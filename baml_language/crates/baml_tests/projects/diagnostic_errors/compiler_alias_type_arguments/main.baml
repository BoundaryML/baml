// Builtin type arguments must be diagnosed rather than erased during lowering.
type InvalidImage = image<string>
type InvalidString = string<int>
type InvalidJson = json<string>
type InvalidMap = map<string>
type InvalidAssociatedBinding = image<Output=string>
