package baml

type StreamResult[Partial any, Final any] struct {
	partial *Partial
	final   *Final
	error   error
}

func (result *StreamResult[Partial, Final]) Partial() Partial {
	return *result.partial
}

func (result *StreamResult[Partial, Final]) Final() Final {
	return *result.final
}

func (result *StreamResult[Partial, Final]) IsFinal() bool {
	return result.final != nil
}

func (result *StreamResult[Partial, Final]) IsPartial() bool {
	return result.partial != nil
}

func (result *StreamResult[Partial, Final]) Error() error {
	return result.error
}
