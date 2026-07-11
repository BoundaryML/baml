class Evaluation<T, V> {
  value: T,
  verdict: V,
}

function judged_by<T, V, E1, E2>(
  run: () -> T throws E1,
  judge: (T) -> V throws E2,
) -> Evaluation<T, V> throws E1 | E2 {
  let value = run();
  Evaluation<T, V> { value: value, verdict: judge(value) }
}
