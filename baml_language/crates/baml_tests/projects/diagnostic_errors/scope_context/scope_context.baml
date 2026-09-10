function arrays_are_not_scope_values() -> boundary.LocalId {
    boundary.id().context(metadata = { "invalid": [1, 2] })
}

function maps_are_not_scope_values() -> boundary.LocalId {
    boundary.id().context(metadata = { "invalid": { "nested": true } })
}

class MetadataObject {
    value string
}

function objects_are_not_scope_values() -> boundary.LocalId {
    boundary.id().context(metadata = { "invalid": MetadataObject { value: "no" } })
}

function identity_must_be_string() -> boundary.LocalId {
    boundary.id().context(distinct_id = 1)
}

function bigint_is_not_a_scope_value(value: bigint) -> boundary.LocalId {
    boundary.id().context(metadata = { "invalid": value })
}

function metadata_cannot_be_null() -> boundary.LocalId {
    boundary.id().context(metadata = null)
}
