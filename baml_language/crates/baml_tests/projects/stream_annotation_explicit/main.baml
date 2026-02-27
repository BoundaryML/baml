// Test explicit user annotations and custom sentinel values.

class Education {
    school string
    year int
}

class ExplicitAnnotations {
    // Custom sentinel: S="Loading...", D=string (default)
    name string @stream.starts_as("Loading...")

    // Not-null: S=never, D=float (default)
    gpa float @stream.not_null

    // Explicit type override (whole elements, not partial):
    // S=[] (container default), D=Education[] (user-specified, no stream_ prefix)
    certifications Education[] @stream.type(Education[])

    // Suppress field entirely: S=never (literal default for never), D=never
    internal string @stream.type(never)
}
