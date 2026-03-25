// Test hover on File type and methods

function TestFileRead() -> string {
    let f<[CURSOR] = baml.fs.open("example.txt");
    f.read()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_fs_file.baml:4:10
// No hover content
