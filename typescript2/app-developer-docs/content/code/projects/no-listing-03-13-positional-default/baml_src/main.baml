// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0005
//       messageContains: "defaulted parameter `right` must be passed by name"
// docs:endmeta
function add(left: int, right: int = 1) -> int {
    left + right
}

function main() -> int {
    add(4, 2)
}
