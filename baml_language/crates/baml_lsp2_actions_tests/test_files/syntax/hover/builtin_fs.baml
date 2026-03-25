// Test hover on file system builtins

function TestFsOpen() -> string {
    let f = baml.fs<[CURSOR].open("example.txt");
    f.read()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_fs.baml:4:20
// No hover content
