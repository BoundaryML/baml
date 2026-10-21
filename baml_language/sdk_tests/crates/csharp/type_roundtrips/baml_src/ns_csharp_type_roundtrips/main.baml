enum DeliveryStatus {
  pending
  delivered
}

type Label = string

class Parcel {
  label Label
  count int
  status DeliveryStatus
  tags string[]
  scores map<string, int>
}

function roundtrip_parcel(value: Parcel) -> Parcel {
  value
}

function roundtrip_union(value: string | int) -> string | int {
  value
}

function roundtrip_literal_union(
  value: int | "fixed",
) -> int | "fixed" {
  value
}

function roundtrip_literal(value: "fixed") -> "fixed" {
  value
}

function roundtrip_bigint(value: bigint) -> bigint {
  value
}

function roundtrip_status_map(
  value: map<string, string>,
) -> map<string, string> {
  value
}

function defaulted_values(
  required: string,
  supplied: string = "engine-default",
  nullable: string? = null,
) -> string[] {
  [required, supplied, nullable ?? "<null>"]
}
