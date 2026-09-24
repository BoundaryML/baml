// Removed stream annotations beside the surviving `@stream.done` / `@@stream.done`.

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

// ── Removed field annotations ───────────────────────────────

class ExplicitAnnotations {
    name string @stream.starts_as("Loading...")
    gpa float @stream.not_null
    certifications Education[] @stream.type(Education[])
    internal string @stream.type(never)
}

// ── Surviving: @stream.done and @@stream.done ───────────────

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

// ── Removed: @@stream.not_null ──────────────────────────────

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

// ── Removed annotations beside surviving ones ───────────────

class AnnotatedResume {
    name string @stream.done
    gpa float @stream.not_null
    title string @stream.starts_as("Loading...")
    internal_id string @stream.type(never)
    token string @stream.done @stream.not_null
}

// ── Removed: @stream.starts_as with an enum value ───────────

enum Status {
    Loading
    Ready
    Error
}

class StatusResponse {
    status Status @stream.starts_as("Status.Loading")
    message string
}

// ── Removed: @stream.type ───────────────────────────────────

class TypeCategoryOverrides {
    items Person[] @stream.type(string)
    maybe string? @stream.type(int)
    internal string @stream.type(never)
    data map<string, int> @stream.type(never)
}

// ── Mixed surviving and removed annotations ─────────────────

class WithDesugaring {
    done_string string @stream.done
    not_null_string string @stream.not_null
    not_null_class Inner @stream.not_null
    done_and_not_null string @stream.done @stream.not_null
    type_never string @stream.type(never)
    type_never_literal "resume" @stream.type(never)
}
