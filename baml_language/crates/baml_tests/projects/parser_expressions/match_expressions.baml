enum Status {
  PENDING
  ACTIVE
  INACTIVE
  SUSPENDED
}

function HandleStatus(status Status) -> string {
  match status {
    Status::PENDING => "Waiting for approval"
    Status::ACTIVE => "Currently active"
    Status::INACTIVE => "Not active"
    Status::SUSPENDED => "Temporarily suspended"
  }
}

function ComplexMatch(value int) -> string {
  match value {
    0 => "zero"
    1 | 2 | 3 => "small"
    4..10 => "medium"
    _ => "large"
  }
}
