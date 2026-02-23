// Cross-file type references: uses Education and Status from defaults.baml.
// Verifies stream expansion works across file boundaries.

class CrossFileUser {
    name string
    edu Education
    status Status
}

type CrossFileAlias = Education | int
