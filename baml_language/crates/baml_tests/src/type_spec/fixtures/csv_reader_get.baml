// R8 (S15 residue): csv reader iteration + Record.get<T>
//
// The S15 residue's `rec.get<int>("n")` failure is upstream of the call:
// `for (let rec in r)` over `Reader` (which `implements iter.Iterable`
// and `iter.Iterator`) must type `rec` as `Record` through the
// iterator protocol (peeling `| iter.Done` off `next`'s result); the
// turbofished `get<int>` then gives `int?`.
function csv_get(rec: baml.csv.Record) -> int throws baml.csv.Error {
    let v = rec.get<int>("n");
     // ^ int | null
    0
}

function csv_iter() -> int throws baml.csv.Error | baml.errors.Io {
    let r = baml.csv.reader("n\n1\n2\n3\n");
     // ^ baml.csv.Reader
    let total = 0;
    for (let rec in r) {
        let x = rec;
         // ^ baml.csv.Record
        total += match (rec.get<int>("n")) {
            null => 0,
            let v: int => v,
        };
    }
    total
}
