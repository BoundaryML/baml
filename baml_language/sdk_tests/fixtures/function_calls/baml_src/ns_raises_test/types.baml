// Throws-contract docstring coverage (32d). Each function/method below pins
// how the Python generator renders a `Raises:` block from the *inferred*
// throws contract (`callable_throws`): union throws, single throws, an
// inferred contract with no written `throws` clause, and a non-throwing
// function (no block). Bodies just `throw` — they are never called; the
// `Raises:` block is produced at codegen time, using **unqualified** names.

/// Raised when a document fails to parse.
class ParseError {
  message string
}

/// Raised when an operation times out.
class TimeoutError {
  ms int
}

/// Load a document from a path.
function LoadDoc(path: string) -> string throws ParseError | TimeoutError {
  throw ParseError { message: path }
}

/// Re-parse a string into a document.
function Reparse(s: string) -> string throws ParseError {
  throw ParseError { message: s }
}

/// Parse without a declared throws clause; the contract is inferred.
function InferredThrow(s: string) -> string {
  throw ParseError { message: s }
}

/// A pure function that never throws.
function PureLen(s: string) -> int {
  4
}

/// Loads documents, with failure modes on its methods.
class DocLoader {
  root string

  /// Load the document at `path`.
  function load(self, path: string) -> string throws ParseError {
    throw ParseError { message: path }
  }

  /// Build a loader, possibly timing out.
  function create() -> DocLoader throws TimeoutError {
    throw TimeoutError { ms: 0 }
  }
}
