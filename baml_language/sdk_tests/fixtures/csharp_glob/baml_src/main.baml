function CompileGlob(pattern: string) -> baml.glob.Glob {
  baml.glob.new(pattern)
}

function EchoGlobScanOptions(options: baml.glob.ScanOptions) -> baml.glob.ScanOptions {
  options
}

type RecursiveNumbers = int | RecursiveNumbers[]

function RoundTripRecursiveNumbers(value: RecursiveNumbers) -> RecursiveNumbers {
  value
}
