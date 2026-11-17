type TisPair = (int, string);

function tis_static(t: (int, string, bool)) -> bool throws never {
    let a = t.0;
     // ^ int
    let b = t.1;
     // ^ string
    let c = t.2;
     // ^ bool
    c
}

function tis_optional(t: (int?, string)?) -> int? throws never {
    let a = t?.1;
     // ^ string | null
    let b = t?.0;
     // ^ int | null
    b
}

function tis_nested(t: ((int, (bool,)), string)) -> bool throws never {
    let a = t.0.1.0;
     // ^ bool
    a
}
