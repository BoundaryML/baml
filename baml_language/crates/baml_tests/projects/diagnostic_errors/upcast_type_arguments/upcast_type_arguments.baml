interface I {}
class C { implements I {} }

function extra_target() -> I throws never { C {}.as<I, MissingType> }
function extra_target_first() -> I throws never { C {}.as<MissingType, I> }
function no_target() -> I throws never { C {}.as<> }
