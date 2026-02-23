// Stream expansion with user-specified streaming attributes.

class WithStartsAs {
    // @stream.starts_as("") → S = "", D = string → "" | string
    loading_name string @stream.starts_as("")

    // @stream.starts_as(null) → explicit null (same as default for scalar)
    explicit_null string @stream.starts_as(null)

    // @stream.starts_as(never) → S = never → field absent until starts → just string
    absent_until_start string @stream.starts_as(never)
}

class WithStreamType {
    // @stream.type(never) → D = never → field omitted entirely
    omitted_field "resume" @stream.type(never)

    // Normal field next to the omitted one
    name string
}

class WithSapFlags {
    // @sap.completed on a literal
    type "resume" @sap.completed

    // @sap.must_start → S = never → absent until parsing begins
    name string @sap.must_start

    // @stream.with_state flag carried to generated field
    status string @stream.with_state
}

class CombinedAttributes {
    // @stream.starts_as("") + @stream.with_state
    name string @stream.starts_as("") @stream.with_state

    // No attributes (default expansion for comparison)
    plain string
}
