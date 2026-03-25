// Test hover on File.close method

function TestFileClose() -> string {
    let f = baml.fs.open("example.txt");
    let content = f.read();
    f.close<[CURSOR]();
    content
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_fs_close.baml:6:12
// No hover content
