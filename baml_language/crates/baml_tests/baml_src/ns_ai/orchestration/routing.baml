class CascadeResult<T> {
  value: T,
  selected: int,
}

function cascade<T, E1, E2>(
  first: () -> T throws E1,
  fallback: () -> T throws E2,
  accept: (T) -> bool throws never,
) -> CascadeResult<T> throws E1 | E2 {
  let value = first();
  if (accept(value)) { return CascadeResult<T> { value: value, selected: 0 }; }
  CascadeResult<T> { value: fallback(), selected: 1 }
}
