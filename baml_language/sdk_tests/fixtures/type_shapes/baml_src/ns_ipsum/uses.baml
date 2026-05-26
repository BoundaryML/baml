// 18a §"Routing rules" — leaf `ipsum`, exercises:
//   - cross-leaf same-package: `root.lorem.Resume` from `ipsum` → `lorem.Resume`
//
// BAML cross-namespace refs use the absolute `root.<ns>.<name>` form.

function RoundTripLoremResumeFromIpsum(r: root.lorem.Resume) -> root.lorem.Resume {
  r
}
