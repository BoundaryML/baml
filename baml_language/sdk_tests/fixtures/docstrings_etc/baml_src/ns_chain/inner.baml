// Innermost frame of a 3-files-deep demo call chain. Same `chain` namespace
// as middle.baml / outer.baml, but a distinct file, so an unwound traceback
// shows three different `.baml` source paths stacked.

class ChainError {
  message string
}

/// Innermost step — throws, so the error unwinds up through middle + outer.
function ChainInner() -> string throws ChainError {
  throw ChainError { message: "boom from the innermost BAML frame" }
}
