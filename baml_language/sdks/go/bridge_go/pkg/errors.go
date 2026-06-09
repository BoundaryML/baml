package pkg

// BamlError is the single error type for all BAML errors surfaced to Go callers.
//
// The engine reports thrown values through a structured BamlOutboundResult
// envelope (see decodeResult/thrownError), so the fully-qualified class name is
// available directly on ClassName (e.g. "baml.errors.InvalidArgument",
// "baml.panics.Cancelled", or a user error class). Callers that need to
// discriminate match on ClassName rather than on a typed Go subtype — see 33b
// for why the typed subtypes were removed.
type BamlError struct {
	// ClassName is the FQN of the thrown value's class, when available.
	ClassName string
	// Message is the rendered error string (class name, message, and traceback).
	Message string
}

func (e *BamlError) Error() string { return e.Message }
