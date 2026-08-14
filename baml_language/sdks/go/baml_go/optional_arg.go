package baml_go

// OptionalArg preserves whether a named callback argument was supplied.
// Its zero value means omitted. A supplied nullable argument remains set even
// when its value is nil, so omission and explicit BAML null stay distinct.
type OptionalArg[T any] struct {
	value T
	set   bool
}

// NewOptionalArg constructs a supplied callback argument. Generated callback
// adapters use this when BAML provided the corresponding named argument.
func NewOptionalArg[T any](value T) OptionalArg[T] {
	return OptionalArg[T]{value: value, set: true}
}

// Get returns the supplied value and whether the argument was supplied.
func (argument OptionalArg[T]) Get() (T, bool) {
	return argument.value, argument.set
}

// IsSet reports whether BAML supplied the argument.
func (argument OptionalArg[T]) IsSet() bool {
	return argument.set
}

// Or returns the supplied value, or fallback when the argument was omitted.
func (argument OptionalArg[T]) Or(fallback T) T {
	if argument.set {
		return argument.value
	}
	return fallback
}
