# BEPv2 Internal Notes

This directory is for maintainers and is not published with the BEP. The
published package consists only of `../README.md` and `../pages/`.

- [Deviations](./deviations.md) lists remaining differences between the spec
  and `crates/baml_tests/baml_src_temp`.
- [Reconciliation](./reconciliation.md) records completed implementation work
  and the latest verification gates.
- [Previous work](./previous-work.md) preserves historical design and branch
  context that current BEP readers do not need.
- [Driver functions vs nominal driver values](./driver-functions-vs-interface-values.md)
  defines the side-by-side `baml_src_temp` / `baml_src_temp2` experiment and
  its decision gate.
- [Tasks, runners, providers, and executable tools](./runner-provider-responsibilities.md)
  records the current runner-oriented direction, naming, ownership boundaries,
  `AnyFunction` tool design, conversation model, and validation invariants.

The API contract lives in the [specification](../pages/specification.md) and
[API reference](../pages/specification/api-reference.md).
