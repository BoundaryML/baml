// Direct cycle: A requires B; B requires A.

interface A requires B {}

interface B requires A {}

//----
//- diagnostics
// <no-diagnostics-expected>
