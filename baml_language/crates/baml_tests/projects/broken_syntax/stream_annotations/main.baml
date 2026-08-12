// Test explicit stream annotations and legacy sugar desugaring.

// ── Shared Types ────────────────────────────────────────────

class Education {
    school string
    year int
}

class Person {
    name string
    age int
}

class Inner {
    value string
}

// ── Explicit Annotations ────────────────────────────────────

class ExplicitAnnotations {
    name string @stream.starts_as("Loading...")
    gpa float @stream.not_null
    certifications Education[] @stream.type(Education[])
    internal string @stream.type(never)
}

// ── Legacy Sugar: @stream.done ──────────────────────────────

class WithDone {
    type "resume" @stream.done
    name string @stream.done
    id string @stream.done @stream.not_null
}

class AtomicPoint {
    @@stream.done
    x float
    y float
}

class AtomicWithOverride {
    @@stream.done
    x float
    y float @stream.starts_as(null)
}

// ── @@stream.not_null ───────────────────────────────────────

class NotNullEducation {
    @@stream.not_null
    school string
    year int
}

enum NotNullStatus {
    @@stream.not_null
    Active
    Inactive
}

class ReferencesNotNull {
    name string
    education NotNullEducation
    status NotNullStatus
    educations NotNullEducation[]
    optional_education NotNullEducation?
}

// ── Annotation-driven Generation ────────────────────────────

class AnnotatedResume {
    name string @stream.done
    gpa float @stream.not_null
    title string @stream.starts_as("Loading...")
    internal_id string @stream.type(never)
    token string @stream.done @stream.not_null
}

// ── @stream.starts_as with Enum Value ───────────────────────

enum Status {
    Loading
    Ready
    Error
}

class StatusResponse {
    status Status @stream.starts_as("Status.Loading")
    message string
}

// ── Type Category Overrides ─────────────────────────────────

class TypeCategoryOverrides {
    items Person[] @stream.type(string)
    maybe string? @stream.type(int)
    internal string @stream.type(never)
    data map<string, int> @stream.type(never)
}

// ── Desugaring ──────────────────────────────────────────────

class WithDesugaring {
    done_string string @stream.done
    not_null_string string @stream.not_null
    not_null_class Inner @stream.not_null
    done_and_not_null string @stream.done @stream.not_null
    type_never string @stream.type(never)
    type_never_literal "resume" @stream.type(never)
}
